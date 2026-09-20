use resona_core::{parser, reducer, Fact, Meta, Rollout, Store};
use serde_json::{json, Value};
use std::io::Write;
const THREAD: &str = "00000000-0000-4000-8000-000000000001";
const BRANCH: &str = "00000000-0000-4000-8000-000000000002";
fn f(kind: &str, turn: &str, at: i64) -> Fact {
    Fact {
        kind: kind.into(),
        turn: Some(turn.into()),
        at: Some(at),
        ..Fact::default()
    }
}
fn roll(id: &str, facts: Vec<Fact>) -> Rollout {
    Rollout {
        provider: "codex".into(),
        id: id.into(),
        meta: Meta {
            thread: THREAD.into(),
            kind: "primary".into(),
            ..Meta::default()
        },
        facts,
        conflict: false,
    }
}
fn completed(id: &str) -> Vec<Fact> {
    vec![
        f("start", id, 1000),
        Fact {
            cumulative: Some(2544),
            ..f("usage", id, 120000)
        },
        Fact {
            duration: Some(120000),
            ttft: Some(7800),
            ..f("finish", id, 121000)
        },
    ]
}
#[test]
fn actual_turns_survive_revert() {
    let turns = reducer::reduce(vec![
        roll(THREAD, [completed("A"), completed("B")].concat()),
        roll(BRANCH, completed("C")),
    ]);
    assert_eq!(turns.len(), 3);
    assert!(turns
        .iter()
        .all(|t| t.owner_thread_id.as_deref() == Some(THREAD)));
    assert_eq!(turns[0].tps, Some(21.2));
}
#[test]
fn copied_turns_do_not_duplicate() {
    assert_eq!(
        reducer::reduce(vec![
            roll(THREAD, completed("A")),
            roll(BRANCH, completed("A"))
        ])
        .len(),
        1
    );
}
#[test]
fn discovery_order_is_irrelevant() {
    let a = roll(THREAD, completed("A"));
    let b = roll(BRANCH, completed("B"));
    assert_eq!(
        reducer::reduce(vec![a.clone(), b.clone()]),
        reducer::reduce(vec![b, a])
    );
}
#[test]
fn repeated_counter_snapshots_contribute_zero() {
    let facts = vec![
        f("start", "A", 1000),
        Fact {
            kind: "counter".into(),
            cumulative: Some(100),
            ..Fact::default()
        },
        Fact {
            kind: "counter".into(),
            cumulative: Some(100),
            ..Fact::default()
        },
        Fact {
            duration: Some(10000),
            ..f("finish", "A", 11000)
        },
    ];
    let turns = reducer::reduce(vec![roll(THREAD, facts)]);
    assert_eq!(turns[0].output_tokens, Some(100));
    assert_eq!(turns[0].tps, Some(10.));
}
#[test]
fn native_usage_does_not_add_old_counters() {
    let mut facts = completed("A");
    facts.insert(
        1,
        Fact {
            kind: "counter".into(),
            cumulative: Some(2544),
            ..Fact::default()
        },
    );
    assert_eq!(
        reducer::reduce(vec![roll(THREAD, facts)])[0].output_tokens,
        Some(2544)
    );
}
#[test]
fn counter_reset_is_unknown() {
    let facts = vec![
        f("start", "A", 1000),
        Fact {
            kind: "counter".into(),
            cumulative: Some(100),
            ..Fact::default()
        },
        Fact {
            kind: "counter".into(),
            cumulative: Some(50),
            ..Fact::default()
        },
        f("finish", "A", 2000),
    ];
    assert_eq!(reducer::reduce(vec![roll(THREAD, facts)])[0].tps, None);
}
#[test]
fn failure_and_abort_have_no_performance() {
    for kind in ["abort", "finish"] {
        let mut facts = completed("A");
        let last = facts.last_mut().unwrap();
        last.kind = kind.into();
        last.failed = true;
        let t = &reducer::reduce(vec![roll(THREAD, facts)])[0];
        assert!(!t.trusted());
        assert_eq!(t.ttft_ms, None);
        assert_eq!(t.tps, None);
    }
}
#[test]
fn different_owner_is_conflict() {
    let a = roll(THREAD, completed("A"));
    let mut b = roll(BRANCH, completed("A"));
    b.meta.thread = BRANCH.into();
    let t = &reducer::reduce(vec![a, b])[0];
    assert_eq!(t.identity_status, "conflict");
    assert!(!t.trusted());
}
#[test]
fn split_start_finish_inherits_context() {
    let mut parent = roll(THREAD, vec![f("start", "A", 1000)]);
    parent.facts[0].end = 50;
    parent.facts[0].ordinal = Some(0);
    let mut child = roll(
        BRANCH,
        vec![Fact {
            duration: Some(120000),
            ttft: Some(7800),
            ..f("finish", "A", 121000)
        }],
    );
    child.meta.base = Some(THREAD.into());
    child.meta.base_byte = Some(50);
    child.meta.base_ordinal = Some(1);
    let t = &reducer::reduce(vec![child, parent])[0];
    assert_eq!(t.started_at_ms, Some(1000));
    assert_eq!(t.ttft_ms, Some(7800));
}
#[test]
fn cycle_does_not_recurse_forever() {
    let mut a = roll(THREAD, completed("A"));
    a.meta.base = Some(THREAD.into());
    a.meta.base_byte = Some(0);
    a.meta.base_ordinal = Some(0);
    assert_eq!(
        reducer::reduce(vec![a])[0].quality_code.as_deref(),
        Some("LINEAGE_INCOMPLETE")
    );
}
#[test]
fn filename_separates_rollout_from_thread() {
    let p = format!("rollout-2026-09-20T12-00-00-{THREAD}_{BRANCH}.jsonl.zst");
    assert_eq!(
        parser::rollout_name(std::path::Path::new(&p)),
        Some((THREAD.into(), BRANCH.into()))
    );
}
#[test]
fn allowlist_never_retains_secrets() {
    for (kind, payload) in [
        (
            "session_meta",
            json!({"id":THREAD,"instructions":"SECRET_SENTINEL","cwd":"SECRET_SENTINEL","source":"cli"}),
        ),
        (
            "event_msg",
            json!({"type":"agent_message","message":"SECRET_SENTINEL"}),
        ),
    ] {
        let fact = parser::codex(&json!({"type":kind,"payload":payload}), 0, 50);
        assert!(!serde_json::to_string(&fact)
            .unwrap()
            .contains("SECRET_SENTINEL"));
    }
}
fn line(kind: &str, payload: Value, ts: &str) -> String {
    format!(
        "{}\n",
        json!({"type":kind,"timestamp":ts,"payload":payload})
    )
}
fn logfile() -> String {
    [
 line("session_meta",json!({"id":THREAD,"source":"cli","history_mode":"legacy"}),"2026-09-20T00:00:00Z"),
 line("event_msg",json!({"type":"task_started","turn_id":"A"}),"2026-09-20T00:00:01Z"),
 line("event_msg",json!({"type":"token_usage_record","turn_id":"A","thread_id":THREAD,"response_id":"R","turn_token_usage":{"output_tokens":2544}}),"2026-09-20T00:02:00Z"),
 line("event_msg",json!({"type":"task_complete","turn_id":"A","duration_ms":120000,"time_to_first_token_ms":7800}),"2026-09-20T00:02:01Z")].concat()
}
#[test]
fn archive_compression_and_duplicate_scan_are_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let active = tmp.path().join(".codex/sessions");
    let archive = tmp.path().join(".codex/archived_sessions");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::create_dir_all(&archive).unwrap();
    let name = format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl");
    let p = active.join(&name);
    std::fs::write(&p, logfile()).unwrap();
    let mut store = Store::open(&tmp.path().join("db"), tmp.path()).unwrap();
    for _ in 0..4 {
        store.scan_step().unwrap();
    }
    assert_eq!(store.all_turns().unwrap().len(), 1);
    std::fs::rename(&p, archive.join(&name)).unwrap();
    store.request_scan();
    for _ in 0..4 {
        store.scan_step().unwrap();
    }
    let compressed = zstd::encode_all(logfile().as_bytes(), 1).unwrap();
    std::fs::write(archive.join(format!("{name}.zst")), compressed).unwrap();
    store.request_scan();
    for _ in 0..4 {
        store.scan_step().unwrap();
    }
    let turns = store.all_turns().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].tps, Some(21.2));
    assert!(turns[0].trusted());
}
#[test]
fn incomplete_line_resumes_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl"));
    let data = logfile();
    std::fs::write(&p, &data[..data.len() - 2]).unwrap();
    let db = tmp.path().join("db");
    {
        let mut s = Store::open(&db, tmp.path()).unwrap();
        for _ in 0..3 {
            s.scan_step().unwrap();
        }
        assert_eq!(s.all_turns().unwrap()[0].status, "running");
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(&data.as_bytes()[data.len() - 2..])
        .unwrap();
    let mut s = Store::open(&db, tmp.path()).unwrap();
    s.recover().unwrap();
    for _ in 0..3 {
        s.scan_step().unwrap();
    }
    assert_eq!(s.all_turns().unwrap()[0].status, "completed");
}
#[test]
fn fixed_storage_settings_reject_unknown_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Store::open(&tmp.path().join("db"), tmp.path()).unwrap();
    let mut json = serde_json::to_value(&s.settings).unwrap();
    json["dataDir"] = json!("/somewhere");
    assert!(serde_json::from_value::<resona_core::Settings>(json).is_err());
}
#[test]
fn ddl_rejects_tps_without_known_denominator() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Store::open(&tmp.path().join("db"), tmp.path()).unwrap();
    let result=s.conn.execute("INSERT INTO turns(provider,turn_key,native_turn_id,thread_kind,status,identity_status,record_source,ttft_source,token_source,tps,parser_version,metric_version,data_revision,updated_at_ms) VALUES ('codex','x','x','primary','completed','verified','parsed','unknown','unknown',5,'v1','v1',1,0)",[]);
    assert!(result.is_err());
}

#[test]
fn native_counter_regression_is_unknown_but_repeated_response_is_idempotent() {
    let usage = |response: &str, n| Fact {
        response: Some(response.into()),
        cumulative: Some(n),
        ..f("usage", "A", 2000)
    };
    for (facts, expected) in [
        (
            vec![usage("r1", 100), usage("r2", 200), usage("r1", 100)],
            Some(200),
        ),
        (vec![usage("r1", 100), usage("r2", 50)], None),
        (vec![usage("r1", 100), usage("r1", 200)], None),
    ] {
        let mut events = vec![f("start", "A", 1000)];
        events.extend(facts);
        events.push(f("finish", "A", 3000));
        assert_eq!(
            reducer::reduce(vec![roll(THREAD, events)])[0].output_tokens,
            expected
        );
    }
}
#[test]
fn copied_legacy_fork_keeps_parent_owner_and_new_child_turn() {
    let parent = roll(THREAD, completed("A"));
    let mut child = roll(BRANCH, [completed("A"), completed("B")].concat());
    child.meta.thread = BRANCH.into();
    child.meta.fork = Some(THREAD.into());
    let turns = reducer::reduce(vec![parent, child]);
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].owner_thread_id.as_deref(), Some(THREAD));
    assert_eq!(turns[1].owner_thread_id.as_deref(), Some(BRANCH));
    assert!(turns.iter().all(|t| t.identity_status == "verified"));
}
#[test]
fn completed_item_is_not_first_token_and_nested_model_is_recognized() {
    let t = parser::codex(
        &json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage"}}}),
        0,
        100,
    );
    assert_eq!(t.kind, "ignored");
    let t = parser::codex(
        &json!({"type":"thread_settings_applied","payload":{"thread_settings":{"model":"test-model"}}}),
        0,
        100,
    );
    assert_eq!(t.model.as_deref(), Some("test-model"));
}
fn drain(store: &mut Store) {
    store.request_scan();
    for _ in 0..100 {
        store.scan_step().unwrap();
        if !store.scanning {
            return;
        }
    }
    panic!("scan did not finish");
}
#[test]
fn same_size_replacement_switches_generation_and_replaces_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let active = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&active).unwrap();
    let path = active.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl"));
    std::fs::write(&path, logfile()).unwrap();
    let mut store = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    drain(&mut store);
    let replacement = active.join("replacement.tmp");
    std::fs::write(&replacement, logfile().replace("\"A\"", "\"B\"")).unwrap();
    std::fs::rename(replacement, &path).unwrap();
    drain(&mut store);
    let turns = store.all_turns().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].native_turn_id, "B");
    assert_eq!(
        store
            .conn
            .query_row("SELECT generation FROM source_files", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn dirty_projection_recovers_after_restart_without_double_counting() {
    let tmp = tempfile::tempdir().unwrap();
    let active = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(
        active.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl")),
        logfile(),
    )
    .unwrap();
    let db = tmp.path().join("data");
    let mut s = Store::open(&db, tmp.path()).unwrap();
    drain(&mut s);
    let before = s.all_turns().unwrap();
    s.conn.execute_batch("DELETE FROM turns; UPDATE rollout_cursors SET dirty=1; UPDATE app_meta SET value_json='true' WHERE key='projection_dirty';").unwrap();
    drop(s);
    let mut s = Store::open(&db, tmp.path()).unwrap();
    s.recover().unwrap();
    assert_eq!(before, s.all_turns().unwrap());
}
#[test]
fn independent_rollouts_do_not_share_pending_counter() {
    let counter = |n| Fact {
        kind: "counter".into(),
        cumulative: Some(n),
        ..Fact::default()
    };
    let a = roll(
        THREAD,
        vec![f("start", "A", 1000), counter(100), f("finish", "A", 2000)],
    );
    let b = roll(
        BRANCH,
        vec![f("start", "B", 1000), counter(300), f("finish", "B", 2000)],
    );
    let turns = reducer::reduce(vec![b, a]);
    assert_eq!(turns[0].output_tokens, Some(100));
    assert_eq!(turns[1].output_tokens, Some(300));
}
#[test]
fn claude_tool_loop_counts_message_snapshots_once() {
    let events = [
        json!({"type":"user","sessionId":"s","uuid":"u","timestamp":"2026-09-20T00:00:00Z","message":{"content":"hello"}}),
        json!({"type":"assistant","sessionId":"s","timestamp":"2026-09-20T00:00:01Z","message":{"id":"m1","content":[{"type":"text","text":"SECRET_SENTINEL"}],"usage":{"output_tokens":10},"stop_reason":"tool_use"}}),
        json!({"type":"user","sessionId":"s","uuid":"tool","message":{"content":[{"type":"tool_result"}]}}),
        json!({"type":"assistant","sessionId":"s","timestamp":"2026-09-20T00:00:02Z","message":{"id":"m1","content":[{"type":"text"}],"usage":{"output_tokens":20},"stop_reason":"tool_use"}}),
        json!({"type":"assistant","sessionId":"s","timestamp":"2026-09-20T00:00:04Z","message":{"id":"m2","content":[{"type":"text"}],"usage":{"output_tokens":20},"stop_reason":"end_turn"}}),
    ];
    let r = Rollout {
        provider: "claude".into(),
        id: "s".into(),
        meta: Meta {
            thread: "s".into(),
            kind: "primary".into(),
            ..Meta::default()
        },
        facts: events.iter().map(|v| parser::claude(v, 0, 100)).collect(),
        conflict: false,
    };
    let turns = reducer::reduce(vec![r]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].output_tokens, Some(40));
    assert_eq!(turns[0].ttft_ms, Some(1000));
    assert_eq!(turns[0].tps, Some(10.));
}

#[test]
fn large_file_finishes_all_batches_before_scan_is_complete() {
    let tmp = tempfile::tempdir().unwrap();
    let active = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&active).unwrap();
    let text = logfile();
    let (first, rest) = text.split_once('\n').unwrap();
    let ignored = line(
        "response_item",
        json!({"type":"message","content":"SECRET_SENTINEL"}),
        "2026-09-20T00:00:00Z",
    );
    let text = format!("{first}\n{}{rest}", ignored.repeat(2100));
    std::fs::write(
        active.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl")),
        text,
    )
    .unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    drain(&mut s);
    assert_eq!(s.all_turns().unwrap()[0].tps, Some(21.2));
}
#[test]
fn explicit_native_owner_conflicts_with_filename_owner() {
    let mut facts = completed("A");
    facts[1].owner = Some(BRANCH.into());
    let turns = reducer::reduce(vec![roll(THREAD, facts)]);
    assert!(!turns[0].trusted());
    assert_eq!(turns[0].tps, None);
}
#[test]
fn legacy_import_includes_committed_wal_without_trusting_old_metrics() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("legacy.db");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE turns(turn_id TEXT,session_key TEXT,started_at_ms INTEGER,completed_at_ms INTEGER,duration_ms INTEGER,ttft_ms INTEGER,output_tokens INTEGER,tps REAL,has_tool INTEGER,status TEXT);INSERT INTO turns VALUES ('old-turn','old-thread',1000,121000,120000,7800,2544,21.2,0,'completed');").unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    assert_eq!(s.import_legacy(&path).unwrap(), 1);
    assert_eq!(s.import_legacy(&path).unwrap(), 0);
    let rows = s.all_turns().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].trusted());
    assert_eq!(rows[0].tps, Some(21.2));
    assert_eq!(
        old.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn reducer_upgrade_rebuilds_existing_projection_from_retained_facts() {
    let tmp = tempfile::tempdir().unwrap();
    let active = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(
        active.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl")),
        logfile(),
    )
    .unwrap();
    let db = tmp.path().join("data");
    let mut s = Store::open(&db, tmp.path()).unwrap();
    drain(&mut s);
    let before = s.all_turns().unwrap();
    s.conn.execute_batch("UPDATE turns SET ttft_ms=NULL; UPDATE app_meta SET value_json='null' WHERE key='projection_version';").unwrap();
    drop(s);
    let mut s = Store::open(&db, tmp.path()).unwrap();
    s.recover().unwrap();
    assert_eq!(before, s.all_turns().unwrap());
    let revision = s.revision().unwrap();
    drop(s);
    let mut s = Store::open(&db, tmp.path()).unwrap();
    s.recover().unwrap();
    assert_eq!(revision, s.revision().unwrap());
}

#[test]
fn reparsed_legacy_row_is_superseded_without_matching_another_provider() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    s.conn.execute_batch("INSERT INTO legacy_rows VALUES ('test','codex','A',NULL,'{}','turn:A','mapped',NULL); INSERT INTO legacy_rows VALUES ('test','claude','A',NULL,'{}','turn:A','mapped',NULL);").unwrap();
    let active = tmp.path().join(".codex/sessions");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(
        active.join(format!("rollout-2026-09-20T00-00-00-{THREAD}.jsonl")),
        logfile(),
    )
    .unwrap();
    drain(&mut s);
    for _ in 0..2 {
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT state FROM legacy_rows WHERE provider='codex'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "superseded"
        );
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT state FROM legacy_rows WHERE provider='claude'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "mapped"
        );
        s.rebuild().unwrap();
    }
}
