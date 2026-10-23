//! Files, diffs and reviews as resources, and update notifications for a
//! subscribed review.

mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test]
async fn change_resources_are_listed_read_and_notify_on_review_activity() {
    let f = Fixture::start().await;
    let mut s = f.session().await;
    let templates = s.ask("resources/templates/list", json!({})).await;
    assert_eq!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let edited = s
        .call(
            "rill_edit",
            json!({ "edits": [{ "op": "set", "path": "a.txt", "content": "one\n" }] }),
        )
        .await;
    let change = edited["change"].as_str().unwrap().to_string();
    let listed = s.ask("resources/list", json!({})).await;
    let uris: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .collect();
    let review_uri = format!("rill://main/changes/{change}/review");
    assert_eq!(
        uris,
        [
            format!("rill://main/changes/{change}/diff"),
            review_uri.clone()
        ]
    );

    let diff = s.ask("resources/read", json!({ "uri": uris[0] })).await;
    let text = diff["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(text.contains("+one"), "{text}");

    s.call("rill_commit", json!({})).await;
    let subscribed = s
        .ask("resources/subscribe", json!({ "uri": review_uri }))
        .await;
    assert!(subscribed["error"].is_null(), "{subscribed}");
    s.call(
        "rill_respond",
        json!({ "actions": [{ "action": "comment", "body": "why one?" }] }),
    )
    .await;
    let updated = s.notification("notifications/resources/updated").await;
    assert_eq!(updated["params"]["uri"], review_uri.as_str());

    s.land("one").await;
    let file = s
        .ask(
            "resources/read",
            json!({ "uri": "rill://main/files/trunk/a.txt" }),
        )
        .await;
    assert_eq!(file["result"]["contents"][0]["text"], "one\n");
    let bad = s
        .ask("resources/read", json!({ "uri": "rill://main/nope" }))
        .await;
    assert!(bad["error"].is_object(), "{bad}");
}
