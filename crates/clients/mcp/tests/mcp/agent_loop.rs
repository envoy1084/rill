//! An agent takes a change from nothing to trunk through MCP alone:
//! read, edit, check, commit, review, land, against a real dev server.

use crate::common::Fixture;
use serde_json::json;

#[tokio::test]
async fn an_agent_reads_edits_checks_commits_reviews_and_lands() {
    let f = Fixture::start().await;
    let mut s = f.session().await;

    let tools = s.ask("tools/list", json!({})).await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names.len(), 14, "{names:?}");

    let context = s.call("rill_context", json!({})).await;
    assert!(context["change"].is_null(), "{context}");
    assert!(context["next"].as_str().unwrap().contains("rill_edit"));

    let edited = s
        .call(
            "rill_edit",
            json!({
                "message": "wip",
                "model": "test-model",
                "edits": [{ "op": "set", "path": "README.md", "content": "# hello\n" }],
            }),
        )
        .await;
    let change = edited["change"].as_str().unwrap().to_string();
    assert!(change.starts_with("ch_"), "{edited}");
    assert_eq!(edited["amended"], false);

    // The session remembers the change: the next edit amends it and keeps
    // the first edit, and reads see the change, not trunk.
    let amended = s
        .call(
            "rill_edit",
            json!({ "edits": [
                { "op": "set", "path": "src/lib.rs", "content": "pub fn one() -> u32 { 1 }\n" },
            ] }),
        )
        .await;
    assert_eq!(amended["change"], change.as_str());
    assert_eq!(amended["amended"], true);
    assert_eq!(amended["files_changed"], 2, "{amended}");

    let read = s
        .call("rill_read", json!({ "paths": ["README.md", "src/lib.rs"] }))
        .await;
    assert_eq!(read["files"][0]["text"], "# hello");
    assert_eq!(read["files"][1]["total_lines"], 1);
    let diff = s.call("rill_read", json!({ "mode": "diff" })).await;
    assert_eq!(diff["files"].as_array().unwrap().len(), 2, "{diff}");
    let found = s.call("rill_search", json!({ "pattern": "fn one" })).await;
    assert_eq!(found["matches"][0]["path"], "src/lib.rs", "{found}");

    let checked = s.call("rill_check", json!({})).await;
    assert_eq!(checked["conflicts"], json!([]));
    assert_eq!(checked["verdict"], "none");

    let committed = s
        .call("rill_commit", json!({ "message": "Add readme and lib" }))
        .await;
    assert_eq!(committed["message"], "Add readme and lib");
    assert!(committed["status"].is_string(), "{committed}");

    let responded = s
        .call(
            "rill_respond",
            json!({ "actions": [
                { "action": "comment", "body": "one is a fine number", "path": "src/lib.rs", "line": 1 },
                { "action": "vote", "label": "Review", "value": 2, "reason": "lgtm" },
                { "action": "vote", "label": "Verified", "value": 1 },
            ] }),
        )
        .await;
    assert_eq!(responded["open_threads"], 1);
    let thread = responded["results"][0]["thread"]
        .as_str()
        .unwrap()
        .to_string();
    s.call(
        "rill_respond",
        json!({ "actions": [{ "action": "resolve", "thread": thread }] }),
    )
    .await;

    let status = s.call("rill_review_status", json!({})).await;
    assert_eq!(status["decision"]["allowed"], true, "{status}");
    assert!(status["next"].as_str().unwrap().contains("rill_land"));

    let landed = s.call("rill_land", json!({ "wait_seconds": 30 })).await;
    assert_eq!(landed["outcome"], "landed", "{landed}");
    assert_eq!(landed["seq"], 1);

    let after = s.call("rill_context", json!({})).await;
    assert!(
        after["change"].is_null(),
        "landing ends the session's change"
    );
    assert_eq!(after["trunk"]["head_seq"], 1);
    let on_trunk = s
        .call(
            "rill_read",
            json!({ "paths": ["README.md"], "at": "trunk" }),
        )
        .await;
    assert_eq!(on_trunk["files"][0]["text"], "# hello");
}
