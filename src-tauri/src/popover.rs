use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize, Rect, WebviewWindow};

#[derive(Default)]
pub struct PopoverState(Mutex<Interaction>);
#[derive(Default)]
struct Interaction {
    blurred: Option<Instant>,
    open_on_release: Option<bool>,
}
impl PopoverState {
    pub fn blur(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.blurred = Some(Instant::now());
        }
    }
    pub fn press(&self, visible: bool) {
        if let Ok(mut state) = self.0.lock() {
            let just_blurred = state
                .blurred
                .is_some_and(|at| at.elapsed() < Duration::from_millis(180));
            state.open_on_release = Some(!visible && !just_blurred);
        }
    }
    pub fn release(&self) -> Option<bool> {
        self.0.lock().ok()?.open_on_release.take()
    }
}

// Opt-in native acceptance trace. Never includes source paths, metrics or session content.
pub fn trace(message: &str) {
    if std::env::var_os("RESONA_WINDOW_TRACE").is_some() {
        eprintln!("resona-window: {message}");
    }
}

fn contains(point: (f64, f64), area: (f64, f64, f64, f64)) -> bool {
    point.0 >= area.0 && point.0 < area.0 + area.2 && point.1 >= area.1 && point.1 < area.1 + area.3
}

fn bounds(tray: (f64, f64, f64, f64), area: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let (left, top, width, height) = area;
    let margin = 8.0;
    let w = 500.0_f64.min((width - margin * 2.0).max(1.0));
    let y = (tray.1 + tray.3).max(top).min(top + height - margin - 1.0);
    let h = 780.0_f64.min((top + height - margin - y).max(1.0));
    let x = (tray.0 + tray.2 / 2.0 - w / 2.0).clamp(left + margin, left + width - w - margin);
    (x, y, w, h)
}

pub fn position_popover(window: &WebviewWindow, rect: Rect) -> tauri::Result<()> {
    // macOS monitor_from_point uses Core Graphics logical coordinates, while
    // tray events and Monitor geometry are physical. Compare in one space.
    let point: PhysicalPosition<f64> = rect.position.to_physical(1.0);
    let size: PhysicalSize<f64> = rect.size.to_physical(1.0);
    let center = (point.x + size.width / 2.0, point.y + size.height / 2.0);
    let monitor = window
        .available_monitors()?
        .into_iter()
        .find(|monitor| {
            contains(
                center,
                (
                    monitor.position().x as f64,
                    monitor.position().y as f64,
                    monitor.size().width as f64,
                    monitor.size().height as f64,
                ),
            )
        })
        .ok_or_else(|| tauri::Error::Io(std::io::Error::other("Tray monitor unavailable")))?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (x, y, width, height) = bounds(
        (
            point.x / scale,
            point.y / scale,
            size.width / scale,
            size.height / scale,
        ),
        (
            area.position.x as f64 / scale,
            area.position.y as f64 / scale,
            area.size.width as f64 / scale,
            area.size.height as f64 / scale,
        ),
    );
    window.set_size(LogicalSize::new(width, height))?;
    window.set_position(PhysicalPosition::new(
        (x * scale).round() as i32,
        (y * scale).round() as i32,
    ))?;
    trace(&format!(
        "tray={point:?} size={size:?}; popover bounds={x},{y},{width},{height} scale={scale}; actual={:?}", window.outer_position()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bounds, contains, PopoverState};
    #[test]
    fn retina_tray_uses_physical_monitor_geometry() {
        // This point lies beyond the logical 1440px edge, but inside the 2x screen.
        assert!(contains((2500., 25.), (0., 0., 2880., 1800.)));
        assert!(!contains((2500., 25.), (0., 0., 1440., 900.)));
        let (x, y, width, _) = bounds((1200., 0., 100., 25.), (0., 25., 1440., 875.));
        assert_eq!(x + width / 2., 1182.); // right edge constraint
        assert_eq!(y, 25.); // no gap below status item
    }
    #[test]
    fn centers_below_tray_without_vertical_gap() {
        let (x, y, width, _) = bounds((900., 0., 100., 25.), (0., 25., 1440., 875.));
        assert_eq!((x + width / 2., y), (950., 25.));
    }

    #[test]
    fn left_click_latches_intent_before_focus_changes() {
        let state = PopoverState::default();
        state.press(false);
        assert_eq!(state.release(), Some(true));
        state.press(true);
        state.blur();
        assert_eq!(state.release(), Some(false));
        assert_eq!(state.release(), None);
        // macOS may report focus loss before the status-item mouse-down.
        state.press(false);
        assert_eq!(state.release(), Some(false));
    }

    #[test]
    fn fits_below_menu_on_small_display() {
        assert_eq!(
            bounds((1200., 0., 100., 25.), (0., 25., 1440., 775.)),
            (932., 25., 500., 767.)
        );
    }
    #[test]
    fn preserves_left_monitor_origin() {
        assert_eq!(
            bounds((-1400., 0., 100., 25.), (-1440., 25., 1440., 875.)),
            (-1432., 25., 500., 780.)
        );
    }
    #[test]
    fn fits_small_work_area() {
        let (x, y, w, h) = bounds((350., 0., 40., 25.), (0., 25., 400., 550.));
        assert!(x >= 8. && x + w <= 392. && y + h <= 567.);
    }
}
