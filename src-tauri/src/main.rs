#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use resona_core::{Bootstrap, Dashboard, Filters, ListRequest, Settings, Store, Turn, TurnPage};
use std::{
    fs::OpenOptions,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutoExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_positioner::{Position, WindowExt};

type Shared = Arc<Mutex<Store>>;
struct Stop(Arc<std::sync::atomic::AtomicBool>);
fn request_quit(app: &tauri::AppHandle) {
    app.state::<Stop>()
        .0
        .store(true, std::sync::atomic::Ordering::Release);
}
#[derive(Clone, Default)]
struct ScanStatus {
    sources: Vec<resona_core::SourceStatus>,
    scanning: bool,
    error: Option<String>,
}
struct QueryContext {
    directory: std::path::PathBuf,
    status: Mutex<ScanStatus>,
    slots: Arc<tokio::sync::Semaphore>,
}
type Queries = Arc<QueryContext>;
async fn read_store<T: Send + 'static>(
    shared: Queries,
    f: impl FnOnce(&Store) -> resona_core::Result<T> + Send + 'static,
) -> Result<T, String> {
    let permit = shared
        .slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "STORE_UNAVAILABLE")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let status = shared
            .status
            .lock()
            .map_err(|_| "STORE_UNAVAILABLE")?
            .clone();
        let reader = Store::read_snapshot(
            &shared.directory,
            status.sources,
            status.scanning,
            status.error,
        )
        .map_err(|e| e.to_string())?;
        f(&reader).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn bootstrap_v1(
    state: State<'_, Queries>,
    app: tauri::AppHandle,
) -> Result<Bootstrap, String> {
    let mut b = read_store(state.inner().clone(), Store::bootstrap).await?;
    b.settings.launch_at_login = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    Ok(b)
}
#[tauri::command]
async fn get_dashboard_v1(
    state: State<'_, Queries>,
    filters: Filters,
) -> Result<Dashboard, String> {
    read_store(state.inner().clone(), move |s| s.dashboard(&filters)).await
}
#[tauri::command]
async fn list_turns_v1(
    state: State<'_, Queries>,
    request: ListRequest,
) -> Result<TurnPage, String> {
    read_store(state.inner().clone(), move |s| s.list_turns(&request)).await
}
#[tauri::command]
async fn get_turn_v1(
    state: State<'_, Queries>,
    provider: String,
    turn_key: String,
) -> Result<Turn, String> {
    read_store(state.inner().clone(), move |s| s.turn(&provider, &turn_key)).await
}
#[tauri::command]
async fn patch_settings_v1(
    state: State<'_, Shared>,
    app: tauri::AppHandle,
    settings: Settings,
) -> Result<Settings, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        settings.validate().map_err(|e| e.to_string())?;
        let mut guard = shared.lock().map_err(|_| "STORE_UNAVAILABLE")?;
        if settings.revision != guard.settings.revision {
            return Err("SETTINGS_CONFLICT".into());
        }
        let previous = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
        if previous != settings.launch_at_login {
            let r = if settings.launch_at_login {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
            r.map_err(|e| e.to_string())?;
        }
        match guard.save_settings(settings) {
            Ok(s) => {
                let _ = app.emit("resona://settings-changed/v1", s.revision);
                Ok(s)
            }
            Err(e) => {
                if previous {
                    let _ = app.autolaunch().enable();
                } else {
                    let _ = app.autolaunch().disable();
                }
                Err(e.to_string())
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn select_source_directory_v1(
    state: State<'_, Shared>,
    app: tauri::AppHandle,
    provider: String,
) -> Result<Option<Settings>, String> {
    if !["codex", "claude"].contains(&provider.as_str()) {
        return Err("INVALID_ARGUMENT".into());
    }
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let file = app
            .dialog()
            .file()
            .set_title(if provider == "codex" {
                "选择 Codex home 目录"
            } else {
                "选择 Claude projects 目录"
            })
            .blocking_pick_folder();
        let Some(file) = file else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let mut store = shared.lock().map_err(|_| "STORE_UNAVAILABLE")?;
        let mut settings = store.settings.clone();
        if provider == "codex" {
            settings.codex_home = path;
        } else {
            settings.claude_projects = path;
        }
        store
            .save_settings(settings)
            .map(Some)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn check_source_v1(state: State<'_, Shared>) -> Result<(), String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        shared
            .lock()
            .map_err(|_| "STORE_UNAVAILABLE")?
            .request_scan();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_directory_v1(
    state: State<'_, Shared>,
    app: tauri::AppHandle,
    target: String,
) -> Result<(), String> {
    let shared = state.inner().clone();
    let p = tauri::async_runtime::spawn_blocking(move || {
        let s = shared.lock().map_err(|_| "STORE_UNAVAILABLE")?;
        match target.as_str() {
            "storage" => Ok(s.directory.clone()),
            "codexHome" => Ok(s.settings.codex_home.clone()),
            "codexActive" => Ok(s.settings.codex_home.join("sessions")),
            "codexArchive" => Ok(s.settings.codex_home.join("archived_sessions")),
            "claudeProjects" => Ok(s.settings.claude_projects.clone()),
            _ => Err("INVALID_ARGUMENT"),
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    app.opener()
        .open_path(p.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn copy_identifier_v1(
    state: State<'_, Queries>,
    app: tauri::AppHandle,
    provider: String,
    turn_key: String,
    field: String,
) -> Result<(), String> {
    let t = get_turn_v1(state, provider, turn_key).await?;
    let value = match field.as_str() {
        "thread" => t.owner_thread_id.ok_or("IDENTITY_UNAVAILABLE")?,
        "turn" => t.native_turn_id,
        _ => return Err("INVALID_ARGUMENT".into()),
    };
    app.clipboard().write_text(value).map_err(|e| e.to_string())
}

fn show(
    app: &tauri::AppHandle,
    destination: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    let label = match destination {
        "popover" => "popover",
        "settings" => "settings",
        "overview" | "turns" => "details",
        _ => return Err("INVALID_ARGUMENT".into()),
    };
    let window = app.get_webview_window(label).ok_or("WINDOW_UNAVAILABLE")?;
    if label != "popover" {
        if let Some(p) = app.get_webview_window("popover") {
            let _ = p.hide();
        }
    }
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    app.emit_to(
        label,
        "resona://navigate/v1",
        serde_json::json!({"destination":destination,"context":payload}),
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn navigate_v1(
    app: tauri::AppHandle,
    destination: String,
    context: Option<serde_json::Value>,
) -> Result<(), String> {
    show(&app, &destination, context.unwrap_or_default())
}
#[tauri::command]
fn hide_popover_v1(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("popover") {
        let _ = w.hide();
    }
}
#[tauri::command]
fn window_visible_v1(window: tauri::WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
}
#[tauri::command]
fn quit_v1(app: tauri::AppHandle) {
    request_quit(&app);
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show(app, "overview", serde_json::Value::Null);
        }))
        .plugin(tauri_plugin_positioner::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent)
                .args(["--background"])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap_v1,
            get_dashboard_v1,
            list_turns_v1,
            get_turn_v1,
            patch_settings_v1,
            select_source_directory_v1,
            check_source_v1,
            open_directory_v1,
            copy_identifier_v1,
            navigate_v1,
            hide_popover_v1,
            quit_v1,
            window_visible_v1
        ])
        .setup(move |app| {
            let home = resona_core::user_home()?;
            let directory = home.join(".resona");
            std::fs::create_dir_all(&directory)?;
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(directory.join("instance.lock"))?;
            fs2::FileExt::try_lock_exclusive(&lock)?;
            let store = Store::open(&directory, &home)?;
            let shared: Shared = Arc::new(Mutex::new(store));
            app.manage(shared.clone());
            let queries: Queries = Arc::new(QueryContext {
                directory: directory.clone(),
                status: Mutex::new(ScanStatus {
                    scanning: true,
                    ..ScanStatus::default()
                }),
                slots: Arc::new(tokio::sync::Semaphore::new(2)),
            });
            app.manage(queries.clone());
            app.manage(lock);
            let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
            app.manage(Stop(stopping.clone()));
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            WebviewWindowBuilder::new(
                app,
                "popover",
                WebviewUrl::App("index.html?view=popover".into()),
            )
            .title("Resona")
            .inner_size(500.0, 780.0)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;
            WebviewWindowBuilder::new(
                app,
                "details",
                WebviewUrl::App("index.html?view=overview".into()),
            )
            .title("Resona · 回响")
            .inner_size(1180.0, 850.0)
            .min_inner_size(900.0, 640.0)
            .visible(false)
            .build()?;
            WebviewWindowBuilder::new(
                app,
                "settings",
                WebviewUrl::App("index.html?view=settings".into()),
            )
            .title("Resona · 设置")
            .inner_size(780.0, 670.0)
            .min_inner_size(680.0, 560.0)
            .visible(false)
            .build()?;
            let detail = MenuItem::with_id(app, "details", "查看详情", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Resona", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&detail, &settings, &quit])?;
            TrayIconBuilder::with_id("resona")
                .icon(
                    app.default_window_icon()
                        .ok_or("Missing application icon")?
                        .clone(),
                )
                .title("—")
                .tooltip("Resona · 感知每一次回响")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "quit" => request_quit(app),
                    "details" => {
                        let _ = show(app, "overview", serde_json::Value::Null);
                    }
                    "settings" => {
                        let _ = show(app, "settings", serde_json::Value::Null);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Some(w) = tray.app_handle().get_webview_window("popover") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.move_window(Position::TrayCenter);
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                use notify::Watcher;
                let (send, receive) = std::sync::mpsc::channel();
                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        if let Ok(event) = event {
                            if event.paths.iter().any(|p| {
                                p.to_string_lossy().ends_with(".jsonl")
                                    || p.to_string_lossy().ends_with(".jsonl.zst")
                            }) {
                                let _ = send.send(());
                            }
                        }
                    })
                    .ok();
                let mut watched_revision = None;
                let mut watched_paths: Vec<std::path::PathBuf> = Vec::new();
                if let Ok(mut store) = shared.lock() {
                    if store.recover().is_err() {
                        store.error = Some("历史状态恢复失败".into());
                    }
                    if store
                        .import_legacy(
                            &home
                                .join("Library/Application Support/CodexLatencyMonitor/monitor.db"),
                        )
                        .is_err()
                    {
                        store.error = Some("旧版历史导入失败；继续从原日志整理".into());
                    }
                }
                while !stopping.load(std::sync::atomic::Ordering::Acquire) {
                    let (mut scanning, mut title) = (false, String::from("—"));
                    if let Ok(mut store) = shared.lock() {
                        if watched_revision != Some(store.settings.revision) {
                            if let Some(watcher) = watcher.as_mut() {
                                for path in watched_paths.drain(..) {
                                    let _ = watcher.unwatch(&path);
                                }
                                for (_, _, root, enabled) in store.roots() {
                                    if enabled
                                        && root.exists()
                                        && watcher
                                            .watch(&root, notify::RecursiveMode::Recursive)
                                            .is_ok()
                                    {
                                        watched_paths.push(root);
                                    }
                                }
                            }
                            watched_revision = Some(store.settings.revision);
                        }
                        let changed = receive.try_iter().count() > 0;
                        if changed && !store.scanning {
                            store.request_scan();
                        }
                        match store.scan_step() {
                            Ok(changed) => {
                                if changed {
                                    let _ = handle.emit(
                                        "resona://data-changed/v1",
                                        store.revision().unwrap_or(0),
                                    );
                                }
                            }
                            Err(_) => store.error = Some("统计更新失败，请检查数据来源".into()),
                        }
                        if let Ok(mut status) = queries.status.lock() {
                            *status = ScanStatus {
                                sources: store.sources.clone(),
                                scanning: store.scanning,
                                error: store.error.clone(),
                            };
                        }
                        scanning = store.scanning;
                        if let Ok(b) = store.bootstrap() {
                            if let Some(t) = b.recent.iter().find(|t| t.trusted()) {
                                let tt = t
                                    .ttft_ms
                                    .map(|n| format!("{:.1}s", n as f64 / 1000.0))
                                    .unwrap_or("N/A".into());
                                let tp =
                                    t.tps.map(|n| format!("{n:.1}t/s")).unwrap_or("N/A".into());
                                title = match b.settings.menu_metric.as_str() {
                                    "tps" => tp,
                                    "both" => format!("{tt} · {tp}"),
                                    _ => tt,
                                };
                                if b.settings.show_provider {
                                    title = format!(
                                        "{} · {title}",
                                        if t.provider == "codex" { "cx" } else { "cc" }
                                    );
                                }
                                if b.settings.show_model {
                                    title =
                                        format!("{} {title}", t.model.as_deref().unwrap_or("N/A"));
                                }
                            }
                        }
                    }
                    if let Some(tray) = handle.tray_by_id("resona") {
                        let _ = tray.set_title(Some(title));
                    }
                    let _ = handle.emit("resona://source-state/v1", ());
                    std::thread::sleep(Duration::from_millis(if scanning { 60 } else { 1500 }));
                }
                handle.exit(0);
            });
            if !std::env::args().any(|a| a == "--background") {
                let _ = show(app.handle(), "overview", serde_json::Value::Null);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "popover" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("Resona desktop runtime failed");
}
