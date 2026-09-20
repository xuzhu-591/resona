use resona_core::{Filters, ListRequest, Store};
fn populate(store: &mut Store, n: usize) {
    let now = resona_core::now_ms();
    let tx = store.conn.transaction().unwrap();
    {
        let mut q=tx.prepare_cached("INSERT INTO turns(provider,turn_key,native_turn_id,owner_thread_id,thread_kind,model,status,identity_status,record_source,started_at_ms,completed_at_ms,duration_ms,ttft_ms,ttft_source,output_tokens,token_source,tps,parser_version,metric_version,data_revision,updated_at_ms) VALUES (?1,?2,?2,'parent','primary','model','completed','verified','parsed',?3,?4,120000,?5,'native',2544,'turn_usage',21.2,'codex-claude-v2','resona-v1',0,0)").unwrap();
        for i in 0..n {
            let end = now - i as i64;
            q.execute(rusqlite::params![
                if i % 2 == 0 { "codex" } else { "claude" },
                format!("t{i:06}"),
                end - 120000,
                end,
                i as i64
            ])
            .unwrap();
        }
    }
    tx.commit().unwrap();
}
fn request() -> ListRequest {
    ListRequest {
        filters: Filters {
            range: "24h".into(),
            ..Filters::default()
        },
        status: None,
        search: None,
        sort: "recent".into(),
        page_size: 20,
        cursor: None,
    }
}
#[test]
fn binned_charts_and_model_summaries_keep_every_sample() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    populate(&mut s, 2000);
    let d = s.dashboard(&request().filters).unwrap();
    assert_eq!(d.summary.completed_count, 2000);
    assert_eq!(d.summary.ttft_p50, Some(0.9995));
    assert_eq!(d.summary.tps_p50, Some(21.2));
    assert!(d.points.is_empty());
    for bins in [&d.ttft_bins, &d.tps_bins] {
        assert_eq!(bins.len(), 48);
        assert_eq!(
            bins.iter()
                .map(|b| b.codex_count + b.claude_count)
                .sum::<usize>(),
            2000
        );
    }
    assert_eq!(
        d.trend_buckets
            .iter()
            .map(|b| b.summary.completed_count)
            .sum::<usize>(),
        2000
    );
    assert_eq!(
        d.models
            .iter()
            .map(|m| m.summary.completed_count)
            .sum::<usize>(),
        2000
    );
}
#[test]
fn pagination_is_bound_to_filters_and_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    populate(&mut s, 48);
    let mut r = request();
    let first = s.list_turns(&r).unwrap();
    r.cursor = first.next_cursor;
    let second = s.list_turns(&r).unwrap();
    assert_eq!(second.items.len(), 20);
    assert!(second
        .items
        .iter()
        .all(|t| first.items.iter().all(|f| f.turn_key != t.turn_key)));
    r.sort = "tps".into();
    assert!(s
        .list_turns(&r)
        .unwrap_err()
        .to_string()
        .contains("INVALID_CURSOR"));
    r.sort = "recent".into();
    s.conn
        .execute(
            "UPDATE app_meta SET value_json='1' WHERE key='data_revision'",
            [],
        )
        .unwrap();
    assert!(s
        .list_turns(&r)
        .unwrap_err()
        .to_string()
        .contains("STALE_CURSOR"));
}
#[test]
fn independent_read_snapshot_survives_writer_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("data");
    let mut s = Store::open(&db, tmp.path()).unwrap();
    populate(&mut s, 1);
    let reader = Store::read_snapshot(&db, vec![], false, None).unwrap();
    assert_eq!(reader.revision().unwrap(), 0);
    s.conn
        .execute(
            "UPDATE app_meta SET value_json='1' WHERE key='data_revision'",
            [],
        )
        .unwrap();
    assert_eq!(reader.revision().unwrap(), 0);
    assert_eq!(
        Store::read_snapshot(&db, vec![], false, None)
            .unwrap()
            .revision()
            .unwrap(),
        1
    );
}
#[test]
fn unavailable_values_do_not_become_zero_or_trusted_samples() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::open(&tmp.path().join("data"), tmp.path()).unwrap();
    populate(&mut s, 2);
    s.conn
        .execute(
            "UPDATE turns SET ttft_ms=NULL,tps=NULL,output_tokens=NULL",
            [],
        )
        .unwrap();
    let d = s.dashboard(&request().filters).unwrap();
    assert_eq!(d.summary.ttft_valid_count, 0);
    assert_eq!(d.summary.ttft_p50, None);
    assert_eq!(d.summary.tps_p50, None);
    s.conn
        .execute(
            "UPDATE turns SET identity_status='legacy_unverified',record_source='legacy'",
            [],
        )
        .unwrap();
    assert_eq!(
        s.dashboard(&request().filters)
            .unwrap()
            .summary
            .completed_count,
        0
    );
}
