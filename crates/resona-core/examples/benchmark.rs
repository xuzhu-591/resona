//! Synthetic query benchmark; never opens the user's statistics or source logs.
use resona_core::{Filters, ListRequest, Store};
fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or("100000".into())
        .parse()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(&temp.path().join("data"), temp.path()).unwrap();
    let now = resona_core::now_ms();
    let tx = store.conn.transaction().unwrap();
    {
        let mut insert=tx.prepare_cached("INSERT INTO turns(provider,turn_key,native_turn_id,owner_thread_id,thread_kind,model,status,identity_status,record_source,started_at_ms,completed_at_ms,duration_ms,ttft_ms,ttft_source,output_tokens,token_source,tps,has_tool,parser_version,metric_version,data_revision,updated_at_ms) VALUES ('codex',?1,?1,'synthetic-thread','primary','test-model','completed','verified','parsed',?2,?3,120000,?4,'native',2544,'turn_usage',21.2,0,'codex-claude-v2','resona-v1',0,0)").unwrap();
        for i in 0..count {
            let end = now - (i as i64 % 600000);
            insert
                .execute(rusqlite::params![
                    format!("t{i}"),
                    end - 120000,
                    end,
                    (i % 30000) as i64
                ])
                .unwrap();
        }
    }
    tx.commit().unwrap();
    let mut dashboard = vec![];
    let mut list = vec![];
    for _ in 0..10 {
        let begin = std::time::Instant::now();
        let result = store
            .dashboard(&Filters {
                range: "7d".into(),
                ..Filters::default()
            })
            .unwrap();
        dashboard.push(begin.elapsed().as_millis());
        assert_eq!(result.summary.completed_count, count);
        assert!(result.trend_buckets.len() <= 240);
        assert!(result.ttft_bins.len() <= 48);
        let begin = std::time::Instant::now();
        let result = store
            .list_turns(&ListRequest {
                filters: Filters {
                    range: "all".into(),
                    ..Filters::default()
                },
                status: None,
                search: None,
                sort: "recent".into(),
                page_size: 20,
                cursor: None,
            })
            .unwrap();
        list.push(begin.elapsed().as_millis());
        assert_eq!(result.total, count);
    }
    println!("rows={count} dashboard_ms={dashboard:?} list_ms={list:?}");
}
