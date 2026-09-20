use crate::{model::*, parser, reducer};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Debug)]
struct FileCheckpoint {
    size: u64,
    offset: u64,
    mtime: String,
    generation: u64,
    device: Option<String>,
    inode: Option<String>,
    prefix: Option<String>,
    checkpoint: Option<String>,
    state: String,
    checked_at: Option<i64>,
}

pub struct Store {
    pub conn: Connection,
    pub directory: PathBuf,
    pub settings: Settings,
    pub sources: Vec<SourceStatus>,
    pub scanning: bool,
    pub error: Option<String>,
    paths: Vec<(String, String, PathBuf)>,
    last_discovery: i64,
    scan_cursor: usize,
    retry_paths: Vec<(String, String, PathBuf)>,
    recent_paths: Vec<(String, String, PathBuf)>,
    recent_probe_at: i64,
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
impl Store {
    pub fn open(directory: &Path, home: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let conn = Connection::open(directory.join("monitor.sqlite3"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match version {
            0 => conn.execute_batch(include_str!("../../../docs/resona-schema-v1.sql"))?,
            1 => {}
            _ => return Err(Error::Invalid("SCHEMA_TOO_NEW".into())),
        }
        let stored: Option<String> = conn
            .query_row(
                "SELECT value_json FROM app_meta WHERE key='settings'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let settings = stored
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_else(|| {
                let mut settings = Settings::for_home(home);
                if let Some(root) = std::env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                {
                    settings.codex_home = root;
                }
                settings
            });
        let mut s = Self {
            conn,
            directory: directory.to_owned(),
            settings,
            sources: vec![],
            scanning: true,
            error: None,
            paths: vec![],
            last_discovery: 0,
            scan_cursor: 0,
            retry_paths: vec![],
            recent_paths: vec![],
            recent_probe_at: 0,
        };
        let previous: String = s.conn.query_row(
            "SELECT value_json FROM app_meta WHERE key='parser_version'",
            [],
            |r| r.get(0),
        )?;
        if previous != serde_json::to_string(PARSER_VERSION)? {
            // Keep old facts/results while replaying. Each file atomically switches generation.
            s.conn.execute("UPDATE source_files SET state='new'", [])?;
            s.conn.execute(
                "UPDATE app_meta SET value_json=?1 WHERE key='parser_version'",
                [serde_json::to_string(PARSER_VERSION)?],
            )?;
        }
        let projection_version =
            serde_json::to_string(&format!("{REDUCER_VERSION}:{METRIC_VERSION}"))?;
        let previous: Option<String> = s
            .conn
            .query_row(
                "SELECT value_json FROM app_meta WHERE key='projection_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if previous.as_ref() != Some(&projection_version) {
            // Version and dirty intent commit together; startup recovery replays retained facts.
            let tx = s.conn.transaction()?;
            tx.execute("UPDATE rollout_cursors SET dirty=1", [])?;
            tx.execute(
                "INSERT OR REPLACE INTO app_meta VALUES ('projection_dirty','true')",
                [],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO app_meta VALUES ('projection_version',?1)",
                [projection_version],
            )?;
            tx.commit()?;
        }
        s.persist_settings()?;
        Ok(s)
    }
    /// One independent WAL snapshot: slow imports never hold the query mutex.
    pub fn read_snapshot(
        directory: &Path,
        sources: Vec<SourceStatus>,
        scanning: bool,
        error: Option<String>,
    ) -> Result<Self> {
        let conn = Connection::open_with_flags(
            directory.join("monitor.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA query_only=ON; BEGIN DEFERRED;")?;
        let json: String = conn.query_row(
            "SELECT value_json FROM app_meta WHERE key='settings'",
            [],
            |r| r.get(0),
        )?;
        Ok(Self {
            conn,
            directory: directory.into(),
            settings: serde_json::from_str(&json)?,
            sources,
            scanning,
            error,
            paths: vec![],
            last_discovery: 0,
            scan_cursor: 0,
            retry_paths: vec![],
            recent_paths: vec![],
            recent_probe_at: 0,
        })
    }
    pub fn turn(&self, provider: &str, key: &str) -> Result<Turn> {
        self.query_turns(
            "provider=?1 AND turn_key=?2",
            rusqlite::params![provider, key],
        )?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Invalid("TURN_NOT_FOUND".into()))
    }
    pub fn revision(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT CAST(value_json AS INTEGER) FROM app_meta WHERE key='data_revision'",
            [],
            |r| r.get::<_, i64>(0).map(|v| v as u64),
        )?)
    }
    pub fn roots(&self) -> Vec<(String, String, PathBuf, bool)> {
        vec![
            (
                "codex".into(),
                "active".into(),
                self.settings.codex_home.join("sessions"),
                self.settings.codex_enabled,
            ),
            (
                "codex".into(),
                "archive".into(),
                self.settings.codex_home.join("archived_sessions"),
                self.settings.codex_enabled,
            ),
            (
                "claude".into(),
                "projects".into(),
                self.settings.claude_projects.clone(),
                self.settings.claude_enabled,
            ),
        ]
    }
    fn persist_settings(&mut self) -> Result<()> {
        let roots = self.roots();
        let settings = serde_json::to_string(&self.settings)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO app_meta(key,value_json) VALUES ('settings',?1)",
            [settings],
        )?;
        tx.execute("UPDATE app_settings SET revision=?1,launch_at_login=?2,menu_metric=?3,show_provider=?4,show_model=?5,theme=?6,default_range=?7,updated_at_ms=?8 WHERE id=1",
            params![self.settings.revision as i64,self.settings.launch_at_login,self.settings.menu_metric,self.settings.show_provider,self.settings.show_model,self.settings.theme,self.settings.default_range,now_ms()])?;
        tx.execute("UPDATE source_roots SET selected=0,enabled=0", [])?;
        for (provider, kind, path, enabled) in roots {
            let path = path.to_string_lossy().to_string();
            let root = digest(path.as_bytes());
            tx.execute("INSERT INTO source_roots(root_id,provider,kind,path,selected,enabled,config_revision,scan_state) VALUES (?1,?2,?3,?4,1,?5,?6,'not_started') ON CONFLICT(root_id) DO UPDATE SET selected=1,enabled=excluded.enabled,config_revision=excluded.config_revision",
                params![root,provider,kind,path,enabled,self.settings.revision as i64])?;
        }
        tx.commit()?;
        self.last_discovery = 0;
        self.scan_cursor = 0;
        Ok(())
    }
    pub fn save_settings(&mut self, mut next: Settings) -> Result<Settings> {
        next.validate()?;
        if next.revision != self.settings.revision {
            return Err(Error::Invalid("SETTINGS_CONFLICT".into()));
        }
        for (old, new) in [
            (&self.settings.codex_home, &next.codex_home),
            (&self.settings.claude_projects, &next.claude_projects),
        ] {
            if old != new {
                let _ = std::fs::read_dir(new)?;
            }
        }
        next.revision += 1;
        let old = self.settings.clone();
        self.settings = next;
        if let Err(e) = self.persist_settings() {
            self.settings = old;
            return Err(e);
        }
        Ok(self.settings.clone())
    }
    pub fn request_scan(&mut self) {
        self.last_discovery = 0;
    }
    fn discover(&mut self) -> Result<()> {
        self.paths.clear();
        self.retry_paths.clear();
        self.sources.clear();
        self.scan_cursor = 0;
        for (provider, kind, path, enabled) in self.roots() {
            let mut status = SourceStatus {
                provider: provider.clone(),
                kind: kind.clone(),
                path: path.to_string_lossy().into(),
                enabled,
                state: "ready".into(),
                ..SourceStatus::default()
            };
            if !enabled {
                status.state = "paused".into();
            } else if !path.exists() {
                status.state = "missing".into();
            } else {
                for entry in walkdir::WalkDir::new(&path)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| e.file_name() != "subagents")
                {
                    match entry {
                        Ok(e) if e.file_type().is_file() => {
                            let name = e.file_name().to_string_lossy();
                            if name.ends_with(".jsonl")
                                || (provider == "codex" && name.ends_with(".jsonl.zst"))
                            {
                                self.paths.push((
                                    provider.clone(),
                                    digest(path.to_string_lossy().as_bytes()),
                                    e.into_path(),
                                ));
                                status.files += 1;
                            }
                        }
                        Err(_) => {
                            status.error_files += 1;
                            status.state = "error".into();
                        }
                        _ => {}
                    }
                }
            }
            self.sources.push(status);
        }
        // Recent files first, so first launch becomes useful before the historical replay finishes.
        self.paths.sort_by_cached_key(|(_, _, p)| {
            std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok())
        });
        self.recent_paths = self.paths.iter().take(24).cloned().collect();
        self.scanning = true;
        self.last_discovery = now_ms();
        Ok(())
    }
    /// Bounded work per tick. All source files are opened read-only.
    pub fn scan_step(&mut self) -> Result<bool> {
        if self.last_discovery == 0 || (now_ms() - self.last_discovery > 60_000 && !self.scanning) {
            self.discover()?;
        }
        if !self.scanning {
            if now_ms() - self.recent_probe_at < 5000 {
                return Ok(false);
            }
            self.recent_probe_at = now_ms();
            self.paths = self.recent_paths.clone();
            self.scan_cursor = 0;
            self.scanning = true;
        }
        if self.paths.is_empty() {
            self.scanning = false;
            return Ok(false);
        }
        let mut changed = false;
        let budget = std::time::Instant::now();
        for _ in 0..24 {
            if self.scan_cursor >= self.paths.len() {
                self.scan_cursor = 0;
                if self.retry_paths.is_empty() {
                    self.scanning = false;
                    break;
                }
                self.paths = std::mem::take(&mut self.retry_paths);
            }
            let (provider, root, path) = self.paths[self.scan_cursor].clone();
            self.scan_cursor += 1;
            match self.ingest(&provider, &root, &path) {
                Ok(did) => {
                    changed |= did;
                    if did
                        && self
                            .conn
                            .query_row(
                                "SELECT state FROM source_files WHERE physical_path=?1",
                                [path.to_string_lossy().as_ref()],
                                |r| r.get::<_, String>(0),
                            )
                            .optional()?
                            .as_deref()
                            == Some("reading")
                    {
                        self.retry_paths
                            .push((provider.clone(), root.clone(), path.clone()));
                    }
                }
                Err(_) => {
                    // Do not include parse error text: it may contain a source line.
                    self.error = Some(format!(
                        "读取失败：{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    if let Some(s) = self
                        .sources
                        .iter_mut()
                        .find(|s| digest(s.path.as_bytes()) == root)
                    {
                        s.error_files += 1;
                        s.state = "error".into();
                    }
                }
            }
            if budget.elapsed().as_millis() > 180 {
                break;
            }
        }
        if changed {
            self.rebuild()?;
        }
        for source in &mut self.sources {
            if source.enabled {
                let errors:i64=self.conn.query_row("SELECT COUNT(*) FROM source_files WHERE root_id=?1 AND last_error_code IS NOT NULL",[digest(source.path.as_bytes())],|r|r.get(0))?;
                source.error_files = source.error_files.max(errors as usize);
                if source.error_files > 0 {
                    source.state = "error".into();
                }
            }
        }
        if !self.scanning {
            for s in &mut self.sources {
                if s.state == "ready" {
                    s.last_success_at_ms = Some(now_ms());
                }
            }
        }
        Ok(changed)
    }
    fn ingest(&mut self, provider: &str, root: &str, path: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(path)?;
        let size = metadata.len();
        let mtime = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let path_string = path.to_string_lossy().to_string();
        let saved = self.conn.query_row("SELECT size_bytes,decoded_offset,COALESCE(mtime_ns,''),generation,device_id,inode_id,prefix_fingerprint,checkpoint_fingerprint,state,last_success_at_ms FROM source_files WHERE physical_path=?1", [&path_string], |r| Ok(FileCheckpoint {
            size:r.get::<_,i64>(0)? as u64,offset:r.get::<_,i64>(1)? as u64,mtime:r.get(2)?,generation:r.get::<_,i64>(3)? as u64,device:r.get(4)?,inode:r.get(5)?,prefix:r.get(6)?,checkpoint:r.get(7)?,state:r.get(8)?,checked_at:r.get(9)?,
        })).optional()?;
        let (device, inode) = file_identity(&metadata);
        if saved.as_ref().is_some_and(|old| {
            (old.state == "ready" || old.state == "error")
                && old.checked_at.is_none_or(|at| now_ms() - at < 86_400_000)
                && old.size == size
                && old.mtime == mtime
                && old.device == device
                && old.inode == inode
        }) {
            return Ok(false);
        }
        let encoding = if path.extension().is_some_and(|e| e == "zst") {
            "zstd"
        } else {
            "jsonl"
        };
        let (thread, rollout) = if provider == "codex" {
            parser::rollout_name(path).ok_or_else(|| Error::Invalid("UNKNOWN_FILENAME".into()))?
        } else {
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (id.clone(), id)
        };
        let reset = if let Some(old) = &saved {
            old.state == "new"
                || ((old.state == "ready" || old.state == "error")
                    && old.checked_at.is_some_and(|at| now_ms() - at >= 86_400_000))
                || size < old.size
                || old.device != device
                || old.inode != inode
                || old.prefix.as_ref().is_some_and(|hash| {
                    fingerprint(path, 0, old.size.min(4096)).ok().as_ref() != Some(hash)
                })
                || old.checkpoint.as_ref().is_some_and(|hash| {
                    let end = if encoding == "zstd" {
                        old.size
                    } else {
                        old.offset
                    };
                    fingerprint(path, end.saturating_sub(4096), end.min(4096))
                        .ok()
                        .as_ref()
                        != Some(hash)
                })
        } else {
            false
        };
        let mut offset = if reset {
            0
        } else {
            saved.as_ref().map_or(0, |old| old.offset)
        };
        let generation = saved
            .as_ref()
            .map_or(1, |old| old.generation + u64::from(reset));
        let mut input: Box<dyn BufRead> = if encoding == "zstd" {
            let mut decoder = zstd::stream::read::Decoder::new(File::open(path)?)?;
            std::io::copy(&mut decoder.by_ref().take(offset), &mut std::io::sink())?;
            Box::new(BufReader::new(decoder))
        } else {
            let mut file = File::open(path)?;
            file.seek(SeekFrom::Start(offset))?;
            Box::new(BufReader::new(file))
        };
        let mut facts = vec![];
        let mut eof = false;
        for _ in 0..1000 {
            let start = offset;
            let (bytes, length, complete) = bounded_line(&mut input)?;
            if length == 0 {
                eof = true;
                break;
            }
            if !complete {
                break;
            }
            offset += length;
            let f = if bytes.is_empty() {
                Fact {
                    kind: "invalid".into(),
                    start,
                    end: offset,
                    ..Fact::default()
                }
            } else {
                match serde_json::from_slice(&bytes) {
                    Ok(v) => {
                        if provider == "codex" {
                            parser::codex(&v, start, offset)
                        } else {
                            parser::claude(&v, start, offset)
                        }
                    }
                    Err(_) => Fact {
                        kind: "invalid".into(),
                        start,
                        end: offset,
                        ..Fact::default()
                    },
                }
            };
            facts.push(f);
        }
        if facts.is_empty() && !reset {
            if saved.is_some() && eof {
                self.conn.execute(
                    "UPDATE source_files SET size_bytes=?1,mtime_ns=?2,state='ready',prefix_fingerprint=?4 WHERE physical_path=?3",
                    params![size as i64, mtime, path_string, fingerprint(path,0,size.min(4096))?],
                )?;
            }
            return Ok(false);
        }
        let invalid = facts.iter().any(|f| f.kind == "invalid");
        let meta = facts.iter().find_map(|f| f.meta.clone()).unwrap_or(Meta {
            thread: thread.clone(),
            kind: if provider == "claude" {
                "primary"
            } else {
                "unknown"
            }
            .into(),
            history_mode: "legacy".into(),
            ..Meta::default()
        });
        let identity = if meta.thread != thread {
            "conflict"
        } else if facts.iter().any(|f| f.meta.is_some()) {
            "verified"
        } else {
            "filename_fallback"
        };
        let prefix = fingerprint(path, 0, size.min(4096))?;
        let checkpoint_end = if encoding == "zstd" { size } else { offset };
        let checkpoint = fingerprint(
            path,
            checkpoint_end.saturating_sub(4096),
            checkpoint_end.min(4096),
        )?;
        let tx = self.conn.transaction()?;
        tx.execute("INSERT INTO rollouts(provider,rollout_id,thread_id,root_session_id,identity_status,thread_kind,history_mode,base_rollout_id,base_end_ordinal,base_end_byte,forked_from_thread_id,forked_from_ordinal,subagent_start_ordinal,lineage_status,parser_version,updated_at_ms,cli_version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) ON CONFLICT(provider,rollout_id) DO NOTHING",
            params![provider,rollout,meta.thread,meta.root,identity,meta.kind,meta.history_mode,meta.base,meta.base_ordinal.map(|n|n as i64),meta.base_byte.map(|n|n as i64),meta.fork,meta.fork_ordinal.map(|n|n as i64),meta.subagent_start.map(|n|n as i64),if meta.base.is_some(){"missing_base"}else{"none"},PARSER_VERSION,now_ms(),meta.version])?;
        tx.execute("INSERT INTO source_files(root_id,provider,rollout_id,physical_path,encoding,generation,size_bytes,mtime_ns,decoded_offset,state) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'ready') ON CONFLICT(physical_path) DO UPDATE SET root_id=excluded.root_id,generation=excluded.generation,size_bytes=excluded.size_bytes,mtime_ns=excluded.mtime_ns,decoded_offset=excluded.decoded_offset,state='ready'",
            params![root,provider,rollout,path_string,encoding,generation as i64,size as i64,&mtime,offset as i64])?;
        let file_id: i64 = tx.query_row(
            "SELECT file_id FROM source_files WHERE physical_path=?1",
            [path_string],
            |r| r.get(0),
        )?;
        tx.execute("UPDATE source_files SET device_id=?1,inode_id=?2,prefix_fingerprint=?3,checkpoint_fingerprint=?4,state=?5 WHERE file_id=?6",params![device,inode,prefix,checkpoint,if eof {"ready"} else {"reading"},file_id])?;
        tx.execute("UPDATE source_files SET last_error_code=CASE WHEN ?1 THEN 'INVALID_SOURCE_RECORD' WHEN ?2 THEN NULL ELSE last_error_code END,last_success_at_ms=CASE WHEN ?3 THEN ?4 ELSE last_success_at_ms END WHERE file_id=?5",params![invalid,reset,eof,now_ms(),file_id])?;
        for f in facts {
            let event_key = f
                .ordinal
                .map_or_else(|| format!("byte:{}", f.start), |n| format!("ordinal:{n}"));
            let json = serde_json::to_string(&f)?;
            let mut semantic = f.clone();
            semantic.start = 0;
            semantic.end = 0;
            let hash = digest(serde_json::to_string(&semantic)?.as_bytes());
            tx.execute("INSERT OR IGNORE INTO event_facts(provider,rollout_id,event_key,variant_hash,ordinal,byte_start,byte_end,at_ms,kind,native_thread_id,native_turn_id,normalized_json,parser_version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![provider,rollout,event_key,hash,f.ordinal.map(|n|n as i64),f.start as i64,f.end as i64,f.at,f.kind,f.owner,f.turn,json,PARSER_VERSION])?;
            let id:i64=tx.query_row("SELECT fact_id FROM event_facts WHERE provider=?1 AND rollout_id=?2 AND event_key=?3 AND variant_hash=?4 AND parser_version=?5",params![provider,rollout,event_key,hash,PARSER_VERSION],|r|r.get(0))?;
            tx.execute("INSERT OR REPLACE INTO file_fact_occurrences(file_id,generation,byte_start,fact_id) VALUES (?1,?2,?3,?4)",params![file_id,generation as i64,f.start as i64,id])?;
        }
        tx.execute("INSERT INTO rollout_cursors(provider,rollout_id,reducer_version,state_json,dirty) VALUES (?1,?2,?3,'{}',1) ON CONFLICT(provider,rollout_id) DO UPDATE SET dirty=1",params![provider,rollout,REDUCER_VERSION])?;
        // Persist dirty intent in the same transaction as offsets; crash recovery always rebuilds it.
        tx.execute(
            "INSERT OR REPLACE INTO app_meta VALUES ('projection_dirty','true')",
            [],
        )?;
        tx.commit()?;
        Ok(true)
    }
    pub fn rebuild(&mut self) -> Result<()> {
        let mut map: BTreeMap<(String, String), Rollout> = BTreeMap::new();
        {
            let mut q=self.conn.prepare("SELECT provider,rollout_id,thread_id,thread_kind,identity_status,base_rollout_id FROM rollouts ORDER BY provider,rollout_id")?;
            for row in q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })? {
                let (provider, id, thread, kind, identity, base) = row?;
                map.insert(
                    (provider.clone(), id.clone()),
                    Rollout {
                        provider,
                        id,
                        meta: Meta {
                            thread,
                            kind,
                            base,
                            ..Meta::default()
                        },
                        facts: vec![],
                        conflict: identity == "conflict",
                    },
                );
            }
        }
        let mut affected: BTreeSet<(String, String)> = self
            .conn
            .prepare("SELECT provider,rollout_id FROM rollout_cursors WHERE dirty=1")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        if affected.is_empty() {
            affected.extend(map.keys().cloned());
        }
        loop {
            let old_len = affected.len();
            let owners: BTreeSet<_> = affected
                .iter()
                .filter_map(|key| {
                    map.get(key)
                        .map(|r| (r.provider.clone(), r.meta.thread.clone()))
                })
                .collect();
            for (key, r) in &map {
                if owners.contains(&(r.provider.clone(), r.meta.thread.clone())) {
                    affected.insert(key.clone());
                }
                if let Some(base) = &r.meta.base {
                    let parent = (r.provider.clone(), base.clone());
                    if affected.contains(key) {
                        affected.insert(parent.clone());
                    }
                    if affected.contains(&parent) {
                        affected.insert(key.clone());
                    }
                }
            }
            // Copied histories sharing a native turn are alternative evidence, not new samples.
            let mut shared=self.conn.prepare("SELECT DISTINCT provider,rollout_id FROM event_facts INDEXED BY idx_facts_turn WHERE provider=?1 AND native_turn_id IN (SELECT DISTINCT native_turn_id FROM event_facts INDEXED BY idx_facts_stream WHERE provider=?1 AND rollout_id=?2 AND native_turn_id IS NOT NULL)")?;
            for (provider, id) in affected.clone() {
                for row in
                    shared.query_map(params![provider, id], |r| Ok((r.get(0)?, r.get(1)?)))?
                {
                    affected.insert(row?);
                }
            }
            if affected.len() == old_len {
                break;
            }
        }
        let mut rolls = vec![];
        {
            let mut q=self.conn.prepare("SELECT event_key,normalized_json FROM event_facts WHERE provider=?1 AND rollout_id=?2 AND parser_version=?3 AND EXISTS (SELECT 1 FROM file_fact_occurrences o JOIN source_files s ON s.file_id=o.file_id AND s.generation=o.generation WHERE o.fact_id=event_facts.fact_id) ORDER BY byte_start,variant_hash")?;
            for key in &affected {
                let Some(mut roll) = map.remove(key) else {
                    continue;
                };
                let mut previous = String::new();
                for row in q.query_map(params![key.0, key.1, PARSER_VERSION], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })? {
                    let (event_key, json) = row?;
                    let fact: Fact = serde_json::from_str(&json)?;
                    if previous == event_key {
                        roll.conflict = true;
                    }
                    previous = event_key;
                    if let Some(meta) = &fact.meta {
                        roll.meta = meta.clone();
                    }
                    roll.facts.push(fact);
                }
                rolls.push(roll);
            }
        }
        let turns = reducer::reduce(rolls);
        let revision = self.revision()? + 1;
        let tx = self.conn.transaction()?;
        // Remove only projections owned by the rebuilt evidence component. The
        // immutable facts remain available across file generations for diagnostics.
        for (provider, id) in &affected {
            tx.execute("DELETE FROM turn_evidence WHERE provider=?1 AND turn_key IN (SELECT CASE WHEN provider='codex' THEN 'turn:' || native_turn_id ELSE native_turn_id END FROM event_facts INDEXED BY idx_facts_stream WHERE provider=?1 AND rollout_id=?2 AND native_turn_id IS NOT NULL)",params![provider,id])?;
            tx.execute("DELETE FROM turns WHERE provider=?1 AND record_source='parsed' AND native_turn_id IN (SELECT native_turn_id FROM event_facts INDEXED BY idx_facts_stream WHERE provider=?1 AND rollout_id=?2 AND native_turn_id IS NOT NULL)",params![provider,id])?;
        }
        for t in turns {
            tx.execute("INSERT INTO turns(provider,turn_key,native_turn_id,owner_thread_id,thread_kind,model,status,identity_status,record_source,started_at_ms,completed_at_ms,first_assistant_at_ms,duration_ms,ttft_ms,ttft_source,output_tokens,token_source,tps,has_tool,quality_code,parser_version,metric_version,data_revision,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24) ON CONFLICT(provider,turn_key) DO UPDATE SET owner_thread_id=excluded.owner_thread_id,thread_kind=excluded.thread_kind,model=excluded.model,status=excluded.status,identity_status=excluded.identity_status,record_source=excluded.record_source,started_at_ms=excluded.started_at_ms,completed_at_ms=excluded.completed_at_ms,first_assistant_at_ms=excluded.first_assistant_at_ms,duration_ms=excluded.duration_ms,ttft_ms=excluded.ttft_ms,ttft_source=excluded.ttft_source,output_tokens=excluded.output_tokens,token_source=excluded.token_source,tps=excluded.tps,has_tool=excluded.has_tool,quality_code=excluded.quality_code,parser_version=excluded.parser_version,metric_version=excluded.metric_version,data_revision=excluded.data_revision,updated_at_ms=excluded.updated_at_ms",
                params![t.provider,t.turn_key,t.native_turn_id,t.owner_thread_id,t.thread_kind,t.model,t.status,t.identity_status,t.record_source,t.started_at_ms,t.completed_at_ms,t.first_assistant_at_ms,t.duration_ms,t.ttft_ms,t.ttft_source,t.output_tokens,t.token_source,t.tps,t.has_tool,t.quality_code,PARSER_VERSION,METRIC_VERSION,revision as i64,now_ms()])?;
        }
        for (provider, id) in affected {
            tx.execute(
                "UPDATE rollout_cursors SET dirty=0,reducer_version=?3 WHERE provider=?1 AND rollout_id=?2",
                params![provider, id, REDUCER_VERSION],
            )?;
        }
        tx.execute("UPDATE legacy_rows SET state='superseded' WHERE state<>'superseded' AND EXISTS (SELECT 1 FROM turns WHERE turns.provider=legacy_rows.provider AND turns.turn_key=legacy_rows.mapped_turn_key AND record_source='parsed')",[])?;
        tx.execute(
            "UPDATE app_meta SET value_json=?1 WHERE key='data_revision'",
            [revision.to_string()],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO app_meta VALUES ('projection_dirty','false')",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn recover(&mut self) -> Result<()> {
        let dirty: Option<String> = self
            .conn
            .query_row(
                "SELECT value_json FROM app_meta WHERE key='projection_dirty'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if dirty.as_deref() == Some("true") {
            self.rebuild()?;
        }
        Ok(())
    }
    pub fn all_turns(&self) -> Result<Vec<Turn>> {
        self.query_turns("1", [])
    }
    pub(crate) fn query_turns(
        &self,
        predicate: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Turn>> {
        self.query_turns_page(
            predicate,
            params,
            "COALESCE(completed_at_ms,started_at_ms) DESC,provider,turn_key",
            None,
            0,
        )
    }
    pub(crate) fn query_turns_page(
        &self,
        predicate: &str,
        params: impl rusqlite::Params,
        order: &str,
        limit: Option<usize>,
        offset: usize,
    ) -> Result<Vec<Turn>> {
        let paging = limit
            .map(|n| format!(" LIMIT {n} OFFSET {offset}"))
            .unwrap_or_default();
        let sql=format!("SELECT provider,turn_key,native_turn_id,owner_thread_id,model,status,thread_kind,CASE WHEN record_source='parsed' AND (parser_version<>'{PARSER_VERSION}' OR metric_version<>'{METRIC_VERSION}') THEN 'unresolved' ELSE identity_status END,record_source,started_at_ms,completed_at_ms,first_assistant_at_ms,duration_ms,ttft_ms,ttft_source,output_tokens,token_source,tps,COALESCE(has_tool,0),quality_code FROM turns WHERE {predicate} ORDER BY {order}{paging}");
        let mut q = self.conn.prepare(&sql)?;
        let rows = q.query_map(params, |r| {
            Ok(Turn {
                provider: r.get(0)?,
                turn_key: r.get(1)?,
                native_turn_id: r.get(2)?,
                owner_thread_id: r.get(3)?,
                model: r.get(4)?,
                status: r.get(5)?,
                thread_kind: r.get(6)?,
                identity_status: r.get(7)?,
                record_source: r.get(8)?,
                started_at_ms: r.get(9)?,
                completed_at_ms: r.get(10)?,
                first_assistant_at_ms: r.get(11)?,
                duration_ms: r.get(12)?,
                ttft_ms: r.get(13)?,
                ttft_source: r.get(14)?,
                output_tokens: r.get(15)?,
                token_source: r.get(16)?,
                tps: r.get(17)?,
                has_tool: r.get(18)?,
                quality_code: r.get(19)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let recent = self.query_turns("(provider,turn_key) IN (SELECT provider,turn_key FROM turns WHERE status != 'running' AND thread_kind = 'primary' ORDER BY COALESCE(completed_at_ms,started_at_ms) DESC LIMIT 6)",[])?;
        let active=self.query_turns("(provider,turn_key) IN (SELECT provider,turn_key FROM turns WHERE status='running' AND thread_kind='primary' ORDER BY started_at_ms DESC LIMIT 10)",[])?;
        Ok(Bootstrap {
            settings: self.settings.clone(),
            storage_directory: self.directory.to_string_lossy().into(),
            sources: self.sources.clone(),
            data_revision: self.revision()?,
            scanning: self.scanning,
            recent,
            active,
            version: env!("CARGO_PKG_VERSION").into(),
            error: self.error.clone(),
        })
    }
}
fn bounded_line(reader: &mut dyn BufRead) -> std::io::Result<(Vec<u8>, u64, bool)> {
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut result = vec![];
    let mut length = 0;
    let mut overflow = false;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok((result, length, false));
        }
        let end = buf.iter().position(|b| *b == b'\n').map(|n| n + 1);
        let n = end.unwrap_or(buf.len());
        length += n as u64;
        if result.len() + n > LIMIT {
            overflow = true;
            result.clear();
        } else if !overflow {
            result.extend_from_slice(&buf[..n]);
        }
        reader.consume(n);
        if end.is_some() {
            return Ok((result, length, true));
        }
    }
}

fn fingerprint(path: &Path, start: u64, length: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(length).read_to_end(&mut bytes)?;
    Ok(digest(&bytes))
}
#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> (Option<String>, Option<String>) {
    use std::os::unix::fs::MetadataExt;
    (
        Some(metadata.dev().to_string()),
        Some(metadata.ino().to_string()),
    )
}
#[cfg(not(unix))]
fn file_identity(_: &std::fs::Metadata) -> (Option<String>, Option<String>) {
    (None, None)
}
