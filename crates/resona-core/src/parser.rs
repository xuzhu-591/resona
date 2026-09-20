//! Allowlist normalization. Message bodies, prompts and tool arguments never leave this adapter.
use crate::model::{Fact, Meta};
use serde_json::Value;

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k)?.as_str().map(str::to_owned)
}
fn number(v: &Value, k: &str) -> Option<i64> {
    v.get(k)?
        .as_i64()
        .filter(|n| *n >= 0 && *n <= 9_007_199_254_740_991)
}
fn at(v: &Value) -> Option<i64> {
    v.as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.timestamp_millis())
}
pub fn codex(v: &Value, start: u64, end: u64) -> Fact {
    let p = &v["payload"];
    let outer = v["type"].as_str().unwrap_or_default();
    let kind = if outer == "event_msg" || outer == "response_item" {
        p["type"].as_str().unwrap_or_default()
    } else {
        outer
    };
    let mut f = Fact {
        kind: "ignored".into(),
        at: at(&v["timestamp"]),
        ordinal: v["ordinal"].as_u64(),
        start,
        end,
        turn: text(p, "turn_id"),
        owner: text(p, "thread_id"),
        ..Fact::default()
    };
    match kind {
        "session_meta" => {
            let source = p["source"].as_str().unwrap_or_default();
            let thread_source = p["thread_source"].as_str().unwrap_or_default();
            let sub = p["source"].get("subagent").is_some()
                || thread_source == "subagent"
                || p.get("parent_thread_id").is_some_and(|v| !v.is_null());
            let primary =
                thread_source == "user" || ["cli", "vscode", "exec", "mcp"].contains(&source);
            f.kind = "meta".into();
            f.meta = Some(Meta {
                thread: text(p, "id").unwrap_or_default(),
                root: text(p, "session_id"),
                kind: if sub {
                    "subagent"
                } else if primary {
                    "primary"
                } else {
                    "unknown"
                }
                .into(),
                base: text(&p["history_base"], "thread_id"),
                base_ordinal: p["history_base"]["end_ordinal_exclusive"].as_u64(),
                base_byte: p["history_base"]["end_byte_offset"].as_u64(),
                fork: text(p, "forked_from_id"),
                fork_ordinal: p["forked_from_ordinal_exclusive"].as_u64(),
                subagent_start: p["subagent_history_start_ordinal"].as_u64(),
                history_mode: text(p, "history_mode")
                    .filter(|s| ["legacy", "paginated"].contains(&s.as_str()))
                    .unwrap_or("legacy".into()),
                version: text(p, "cli_version"),
            });
        }
        "task_started" => {
            f.kind = "start".into();
            f.at = f.at.or_else(|| number(p, "started_at").map(|n| n * 1000));
        }
        "task_complete" => {
            f.kind = "finish".into();
            f.duration = number(p, "duration_ms");
            f.ttft = number(p, "time_to_first_token_ms");
            f.failed = p.get("error").is_some_and(|v| !v.is_null());
            f.at = f.at.or_else(|| number(p, "completed_at").map(|n| n * 1000));
        }
        "turn_aborted" => f.kind = "abort".into(),
        "turn_context" | "thread_settings_applied" => {
            f.kind = "model".into();
            f.model = text(p, "model").or_else(|| text(&p["thread_settings"], "model"));
        }
        "agent_reasoning" | "agent_message" => f.kind = "assistant".into(),
        "token_usage_record" => {
            f.kind = "usage".into();
            f.cumulative = number(&p["turn_token_usage"], "output_tokens");
            f.output = number(&p["usage"], "output_tokens");
            f.response = text(p, "response_id");
        }
        "token_count" => {
            f.kind = "counter".into();
            f.cumulative = number(&p["info"]["total_token_usage"], "output_tokens");
        }
        "function_call"
        | "custom_tool_call"
        | "exec_command_begin"
        | "patch_apply_begin"
        | "mcp_tool_call_begin" => {
            f.kind = "tool".into();
            f.tool = true;
        }
        "item_started" | "item_completed" => {
            let item = p["item"]["type"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase();
            if [
                "commandexecution",
                "filechange",
                "mcptoolcall",
                "websearch",
                "function_call",
            ]
            .contains(&item.as_str())
            {
                f.kind = "tool".into();
                f.tool = true;
            }
        }
        _ => {}
    }
    f
}
pub fn claude(v: &Value, start: u64, end: u64) -> Fact {
    let mut f = Fact {
        kind: "ignored".into(),
        at: at(&v["timestamp"]),
        start,
        end,
        ..Fact::default()
    };
    if ["isSidechain", "isMeta", "isCompactSummary"]
        .iter()
        .any(|k| v[*k].as_bool() == Some(true))
    {
        return f;
    }
    let session = text(v, "sessionId");
    let message = &v["message"];
    let content = message["content"].as_array();
    let has = |kind: &str| {
        content.is_some_and(|items| items.iter().any(|i| i["type"].as_str() == Some(kind)))
    };
    match v["type"].as_str().unwrap_or_default() {
        "user" if !has("tool_result") => {
            if let (Some(s), Some(id)) = (session.as_ref(), text(v, "uuid")) {
                f.kind = "start".into();
                f.turn = Some(format!("session:{s}:user:{id}"));
                f.owner = Some(s.clone());
            }
        }
        "assistant" => {
            f.kind = if message["stop_reason"].as_str() == Some("end_turn") {
                "claude_finish"
            } else {
                "claude_assistant"
            }
            .into();
            f.owner = session.clone();
            f.model = text(message, "model");
            f.output = number(&message["usage"], "output_tokens");
            f.response = text(message, "id").or_else(|| text(v, "uuid"));
            f.tool = has("tool_use");
            if !has("thinking") && !has("text") {
                f.at = None;
            }
        }
        _ => {}
    }
    // Session identity is retained independently of message content.
    if let Some(s) = session {
        f.meta = Some(Meta {
            thread: s,
            kind: "primary".into(),
            history_mode: "legacy".into(),
            ..Meta::default()
        });
    }
    f
}

/// Parse both ordinary and reverted filenames, never the last UUID as the thread ID.
pub fn rollout_name(path: &std::path::Path) -> Option<(String, String)> {
    let name = path
        .file_name()?
        .to_str()?
        .trim_end_matches(".zst")
        .strip_suffix(".jsonl")?;
    let rest = name.strip_prefix("rollout-")?;
    if rest.len() < 56 {
        return None;
    }
    let tail = rest.get(20..)?;
    let mut ids = tail.split('_');
    let thread = ids.next()?;
    let id = ids.next().unwrap_or(thread);
    if ids.next().is_some()
        || uuid::Uuid::parse_str(thread).is_err()
        || uuid::Uuid::parse_str(id).is_err()
    {
        return None;
    }
    Some((thread.into(), id.into()))
}
