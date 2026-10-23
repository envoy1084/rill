//! End-to-end: an in-process dev server driven through the CLI as a
//! library. Covers the create / checkout / new / submit / vote / land /
//! sync loop and the JSON output agents rely on.

mod common;

use common::{approve, Server};
use serde_json::Value;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_checkout_land_and_sync() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    // Repository.
    let created = server
        .json(home, &["repo", "create", "e2e", "--description", "test"])
        .await;
    assert_eq!(created["name"], "e2e");
    let repos = server.json(home, &["repo", "list"]).await;
    assert_eq!(repos["items"][0]["name"], "e2e");
    let (code, _) = server.rill(home, &["repo", "show", "--repo", "nope"]).await;
    assert_eq!(code, 1, "missing repo is an error");

    // Checkout an empty repository and write files.
    let co = home.join("a");
    let checkout = server
        .json(home, &["checkout", "e2e", co.to_str().unwrap()])
        .await;
    assert_eq!(checkout["files"], 0);
    assert!(checkout["workspace"].as_str().unwrap().starts_with("ws_"));
    std::fs::write(co.join("README.md"), "# e2e\n").unwrap();
    std::fs::create_dir_all(co.join("src")).unwrap();
    std::fs::write(co.join("src/lib.rs"), "pub fn one() -> u32 { 1 }\n").unwrap();

    let status = server.json(&co, &["status"]).await;
    assert_eq!(status["changes"].as_array().unwrap().len(), 2);
    assert_eq!(status["changes"][0]["kind"], "added");
    let diff = server.json(&co, &["diff"]).await;
    assert!(diff["files"][0]["unified"]
        .as_str()
        .unwrap()
        .contains("+# e2e"));

    // New change, submit, vote, land.
    let new = server.json(&co, &["new", "-m", "first change"]).await;
    let change_id = new["change_id"].as_str().unwrap().to_string();
    assert!(change_id.starts_with("ch_"));
    assert_eq!(new["version"]["file_changes"].as_array().unwrap().len(), 2);
    assert_eq!(new["version"]["provenance"]["tool"], "rill-cli");
    let status = server.json(&co, &["status"]).await;
    assert_eq!(status["working_change"], change_id);

    let shown = server.json(&co, &["show", &change_id]).await;
    assert_eq!(shown["summary"], "first change");

    let submitted = server.json(&co, &["submit"]).await;
    assert_eq!(submitted["change_id"], change_id);
    let voted = server
        .json(
            &co,
            &["review", "vote", &change_id, "Review", "+2", "-m", "lgtm"],
        )
        .await;
    assert_eq!(voted["votes"][0]["value"], 2);
    let evaluated = server.json(&co, &["review", "evaluate", &change_id]).await;
    assert_eq!(evaluated["allowed"], true, "{evaluated}");
    let landed = server
        .json(&co, &["land", "--wait", "--timeout", "30"])
        .await;
    assert_eq!(landed["status"], "landed");
    assert_eq!(landed["seq"], 1);

    // History and content from outside any checkout.
    let log = server.json(home, &["--repo", "e2e", "log"]).await;
    assert_eq!(log["entries"][0]["trunk_seq"], 1);
    assert_eq!(log["entries"][0]["summary"], "first change");
    let cat = server
        .ok(home, &["--repo", "e2e", "cat", "src/lib.rs"])
        .await;
    assert_eq!(cat, "pub fn one() -> u32 { 1 }\n");
    let ls = server.json(home, &["--repo", "e2e", "ls"]).await;
    assert_eq!(ls["entries"][1]["name"], "src");
    let grep = server
        .json(home, &["--repo", "e2e", "grep", "fn one"])
        .await;
    assert_eq!(grep["matches"][0]["path"], "src/lib.rs");
    let trunk = server.json(home, &["--repo", "e2e", "trunk"]).await;
    assert_eq!(trunk["head_seq"], 1);
    let refs = server.json(home, &["--repo", "e2e", "ref", "list"]).await;
    assert_eq!(refs["refs"][0]["name"], "trunk");
    let landings = server
        .json(home, &["--repo", "e2e", "trunk", "landings"])
        .await;
    assert_eq!(landings["landings"][0]["event"]["seq"], 1);
    let me = server.json(home, &["whoami"]).await;
    assert!(me["principal"]["name"].is_string());

    // Sync the first checkout onto trunk after landing.
    let synced = server.json(&co, &["sync"]).await;
    assert_eq!(synced["base"], "trunk");
    assert!(synced["conflicts"].as_array().unwrap().is_empty());
    let status = server.json(&co, &["status"]).await;
    assert!(status["changes"].as_array().unwrap().is_empty(), "{status}");

    // A second checkout lands another change; the first picks it up.
    let co2 = home.join("b");
    server
        .json(home, &["checkout", "e2e", co2.to_str().unwrap()])
        .await;
    assert!(co2.join("README.md").exists());
    std::fs::write(co2.join("src/two.rs"), "pub fn two() -> u32 { 2 }\n").unwrap();
    let second = server.json(&co2, &["new", "-m", "second change"]).await;
    let second_id = second["change_id"].as_str().unwrap().to_string();
    server.json(&co2, &["submit"]).await;
    server
        .json(&co2, &["review", "vote", &second_id, "Review", "+2"])
        .await;
    let landed = server.json(&co2, &["land", "--wait"]).await;
    assert_eq!(landed["seq"], 2);

    let synced = server.json(&co, &["sync"]).await;
    assert_eq!(synced["updated"], serde_json::json!(["src/two.rs"]));
    assert!(co.join("src/two.rs").exists());
    let log = server.json(&co, &["log", "-n", "1"]).await;
    assert_eq!(log["entries"][0]["trunk_seq"], 2);

    // Intents and the stack view.
    let intent = server
        .json(&co, &["intent", "create", "Ship it", "--kind", "chore"])
        .await;
    let intent_id = intent["id"].as_str().unwrap().to_string();
    std::fs::write(co.join("NOTES.md"), "notes\n").unwrap();
    let third = server
        .json(&co, &["new", "-m", "third", "--intent", &intent_id])
        .await;
    assert_eq!(third["version"]["intent"], intent_id);
    let stack = server.json(&co, &["stack", "show"]).await;
    assert_eq!(stack["changes"].as_array().unwrap().len(), 1);
    let linked = server.json(&co, &["intent", "show", &intent_id]).await;
    assert_eq!(linked["changes"][0], third["change_id"]);

    // Human output renders without --json too.
    let text = server.ok(&co, &["log"]).await;
    assert!(text.contains("second change"));
    let text = server.ok(&co, &["status"]).await;
    assert!(text.contains("Working change"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conflicting_sync_exits_three() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "c"]).await;
    let co = home.join("a");
    server
        .json(home, &["checkout", "c", co.to_str().unwrap()])
        .await;
    std::fs::write(co.join("f.txt"), "one\ntwo\nthree\n").unwrap();
    let first = server.json(&co, &["new", "-m", "base"]).await;
    let id = first["change_id"].as_str().unwrap().to_string();
    server.json(&co, &["submit"]).await;
    approve(&server, &co, &id).await;
    server.json(&co, &["land", "--wait"]).await;
    server.json(&co, &["sync"]).await;

    // Land a conflicting edit from a second checkout.
    let co2 = home.join("b");
    server
        .json(home, &["checkout", "c", co2.to_str().unwrap()])
        .await;
    std::fs::write(co2.join("f.txt"), "one\nTWO-b\nthree\n").unwrap();
    let second = server.json(&co2, &["new", "-m", "b edit"]).await;
    let id2 = second["change_id"].as_str().unwrap().to_string();
    server.json(&co2, &["submit"]).await;
    approve(&server, &co2, &id2).await;
    server.json(&co2, &["land", "--wait"]).await;

    // The first checkout edits the same line locally, then syncs.
    std::fs::write(co.join("f.txt"), "one\nTWO-a\nthree\n").unwrap();
    let (code, out) = server.rill(&co, &["--json", "sync"]).await;
    assert_eq!(code, 3, "{out}");
    let value: Value = serde_json::from_str(&out).unwrap();
    let value = value["result"].clone();
    assert_eq!(value["conflicts"], serde_json::json!(["f.txt"]));
    let content = std::fs::read_to_string(co.join("f.txt")).unwrap();
    assert!(
        content.contains("TWO-a") && content.contains("TWO-b"),
        "{content}"
    );

    let listed = server.json(&co, &["conflicts"]).await;
    assert_eq!(listed["conflicts"], serde_json::json!(["f.txt"]));
    std::fs::write(co.join("f.txt"), "one\nTWO-ab\nthree\n").unwrap();
    let resolved = server.json(&co, &["resolve", "f.txt"]).await;
    assert_eq!(resolved["remaining"], 0);
    let status = server.json(&co, &["status"]).await;
    assert_eq!(status["changes"][0]["kind"], "modified");
    let amended = server.json(&co, &["new", "-m", "merged"]).await;
    assert_eq!(amended["has_conflicts"], false);
}
