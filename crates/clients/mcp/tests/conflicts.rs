//! Trunk moves under an agent's change: sync records the conflict, the
//! agent reads both sides and resolves it, and the change lands.

mod common;

use common::Fixture;
use serde_json::json;

fn set(path: &str, content: &str) -> serde_json::Value {
    json!({ "edits": [{ "op": "set", "path": path, "content": content }] })
}

#[tokio::test]
async fn a_conflict_after_sync_is_inspected_resolved_and_landed() {
    let f = Fixture::start().await;
    let mut setup = f.session().await;
    setup.call("rill_edit", set("README.md", "base\n")).await;
    setup.land("base").await;

    let mut ours = f.session().await;
    let mut theirs = f.session().await;
    ours.call("rill_edit", set("README.md", "ours\n")).await;
    theirs.call("rill_edit", set("README.md", "theirs\n")).await;
    theirs.land("theirs").await;

    let synced = ours.call("rill_sync", json!({})).await;
    assert_eq!(synced["moved"], true);
    assert_eq!(synced["conflicts"], json!(["README.md"]), "{synced}");
    assert!(synced["next"].as_str().unwrap().contains("rill_conflicts"));

    let conflicts = ours.call("rill_conflicts", json!({})).await;
    let sides: Vec<&str> = conflicts["conflicts"][0]["sides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["text"].as_str().unwrap())
        .collect();
    assert!(
        sides.contains(&"ours\n") && sides.contains(&"theirs\n"),
        "{conflicts}"
    );
    assert_eq!(conflicts["conflicts"][0]["bases"][0]["text"], "base\n");

    let resolved = ours
        .call(
            "rill_resolve",
            json!({ "resolutions": [{ "path": "README.md", "content": "ours and theirs\n" }] }),
        )
        .await;
    assert_eq!(resolved["conflicts"], json!([]), "{resolved}");
    let landed = ours.land("ours, merged").await;
    assert_eq!(landed["seq"], 3);
    let read = ours
        .call(
            "rill_read",
            json!({ "paths": ["README.md"], "at": "trunk" }),
        )
        .await;
    assert_eq!(read["files"][0]["text"], "ours and theirs");
}
