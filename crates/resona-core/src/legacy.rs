//! One-time read-only import. SQLite Backup includes committed WAL pages.
use crate::{Error, Result, Store};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, time::Duration};
impl Store {
    pub fn import_legacy(&mut self, path: &Path) -> Result<usize> {
        if !path.exists() {
            return Ok(0);
        }
        let done: Option<String> = self
            .conn
            .query_row(
                "SELECT value_json FROM app_meta WHERE key='legacy_imported'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if done.as_deref() == Some("true") {
            return Ok(0);
        }
        let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.busy_timeout(Duration::from_secs(5))?;
        let staging = self.directory.join("staging");
        std::fs::create_dir_all(&staging)?;
        let snapshot = staging.join("legacy-import.sqlite3");
        let mut copy = Connection::open(&snapshot)?;
        {
            let backup = rusqlite::backup::Backup::new(&source, &mut copy)?;
            backup.run_to_completion(100, Duration::from_millis(5), None)?;
        }
        let mut hasher = Sha256::new();
        let mut input = std::fs::File::open(&snapshot)?;
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let import_id = format!("{:x}", hasher.finalize());
        let columns = copy
            .prepare("PRAGMA table_info(turns)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !columns.contains(&"turn_id".to_owned()) {
            return Err(Error::Invalid("LEGACY_SCHEMA_UNSUPPORTED".into()));
        }
        let provider = if columns.contains(&"provider".to_owned()) {
            "provider"
        } else {
            "'codex'"
        };
        let model = if columns.contains(&"model".to_owned()) {
            "model"
        } else {
            "NULL"
        };
        let sql=format!("SELECT turn_id,session_key,{provider},{model},started_at_ms,completed_at_ms,duration_ms,ttft_ms,output_tokens,tps,has_tool,status FROM turns");
        let mut q = copy.prepare(&sql)?;
        let mut rows = q.query([])?;
        let tx = self.conn.transaction()?;
        let mut count = 0;
        while let Some(r) = rows.next()? {
            let id: String = r.get(0)?;
            let owner: Option<String> = r.get(1)?;
            let provider: String = r.get(2)?;
            if !["codex", "claude"].contains(&provider.as_str()) {
                continue;
            }
            let key = if provider == "codex" {
                format!("turn:{id}")
            } else {
                let parts: Vec<_> = id.splitn(3, ':').collect();
                if parts.len() == 3 && parts[0] == "claude" {
                    format!("session:{}:user:{}", parts[1], parts[2])
                } else {
                    format!("legacy:{id}")
                }
            };
            let model: Option<String> = r.get(3)?;
            let start: Option<i64> = r.get(4)?;
            let end: Option<i64> = r.get(5)?;
            let duration: Option<i64> = r.get::<_, Option<i64>>(6)?.filter(|n| *n >= 0);
            let status: String = r.get(11)?;
            let ttft = if status == "completed" {
                r.get::<_, Option<i64>>(7)?.filter(|n| *n >= 0)
            } else {
                None
            };
            let tokens = r.get::<_, Option<i64>>(8)?.filter(|n| *n >= 0);
            let tps = if status == "completed"
                && tokens.is_some_and(|n| n > 0)
                && duration.is_some_and(|n| n > 0)
            {
                r.get::<_, Option<f64>>(9)?
                    .filter(|n| n.is_finite() && *n >= 0.)
            } else {
                None
            };
            let tool: bool = r.get(10)?;
            let status =
                if ["completed", "aborted", "failed", "incomplete"].contains(&status.as_str()) {
                    status
                } else {
                    "incomplete".into()
                };
            let normalized = serde_json::json!({"turnId":id,"provider":provider,"model":model,"startedAtMs":start,"completedAtMs":end,"durationMs":duration,"ttftMs":ttft,"outputTokens":tokens,"tps":tps,"status":status});
            tx.execute("INSERT OR IGNORE INTO legacy_rows(import_id,provider,legacy_turn_id,legacy_session_key,normalized_json,mapped_turn_key,state,reason_code) VALUES (?6,?1,?2,?3,?4,?5,'mapped','LEGACY_UNVERIFIED')",params![provider,id,owner,normalized.to_string(),key,import_id])?;
            count+=tx.execute("INSERT OR IGNORE INTO turns(provider,turn_key,native_turn_id,owner_thread_id,thread_kind,model,status,identity_status,record_source,started_at_ms,completed_at_ms,duration_ms,ttft_ms,ttft_source,output_tokens,token_source,tps,has_tool,quality_code,parser_version,metric_version,data_revision,updated_at_ms) VALUES (?1,?2,?3,?4,'unknown',?5,?6,'legacy_unverified','legacy',?7,?8,?9,?10,'legacy',?11,'legacy',?12,?13,'LEGACY_UNVERIFIED','legacy','legacy',0,?14)",params![provider,key,id,owner,model,status,start,end,duration,ttft,tokens,tps,tool,crate::now_ms()])?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO app_meta VALUES ('legacy_imported','true')",
            [],
        )?;
        tx.commit()?;
        drop(rows);
        drop(q);
        drop(copy);
        drop(source);
        std::fs::remove_file(snapshot)?;
        Ok(count)
    }
}
