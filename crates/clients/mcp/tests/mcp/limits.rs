//! What keeps an agent's context and loops in check: response budgets
//! with continuation cursors, and the stop hint for repeated failures.

use crate::common::Fixture;
use serde_json::json;

#[tokio::test]
async fn a_large_file_is_read_in_budgeted_pages_that_join_up() {
    let f = Fixture::start().await;
    let mut s = f.session().await;
    let content: String = (1..=2_000).map(|i| format!("line {i}\n")).collect();
    s.call(
        "rill_edit",
        json!({ "edits": [{ "op": "set", "path": "big.txt", "content": content }] }),
    )
    .await;

    let mut lines = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let mut args = json!({ "paths": ["big.txt"], "max_tokens": 1_000 });
        if let Some(c) = &cursor {
            args["cursor"] = json!(c);
        }
        let page = s.call("rill_read", args).await;
        let text = page["files"][0]["text"].as_str().unwrap();
        assert!(
            text.len() <= 4_000,
            "page over budget: {} chars",
            text.len()
        );
        lines.extend(text.lines().map(str::to_string));
        pages += 1;
        match page["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }
    assert!(pages > 1);
    let expected: Vec<String> = (1..=2_000).map(|i| format!("line {i}")).collect();
    assert_eq!(lines, expected);
}

#[tokio::test]
async fn repeating_a_failing_call_earns_a_stop_hint() {
    let f = Fixture::start().await;
    let mut s = f.session().await;
    let args = json!({ "change": "ch_00000000000000000000000000000000" });
    for attempt in 1..=3 {
        let (error, failed) = s.call_raw("rill_review_status", args.clone()).await;
        assert!(failed);
        assert_eq!(
            error["error"]["stop"].is_string(),
            attempt == 3,
            "attempt {attempt}: {error}"
        );
    }
}
