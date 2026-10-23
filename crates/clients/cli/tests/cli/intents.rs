//! `rill intent scope | approve | explain`: a change committed under an
//! approved intent inherits its approval inside the scope.

use crate::common::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_under_an_approved_intent_is_in_scope_until_it_leaves_it() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "plan"]).await;
    let co = home.join("co");
    server
        .json(home, &["checkout", "plan", co.to_str().unwrap()])
        .await;

    let intent = server
        .json(
            &co,
            &[
                "intent",
                "create",
                "Migrate the gateway",
                "--kind",
                "refactor",
            ],
        )
        .await;
    let id = intent["id"].as_str().unwrap().to_string();
    let scoped = server
        .json(
            &co,
            &[
                "intent",
                "scope",
                &id,
                "--path",
                "gw/**",
                "--forbid",
                "gw/secrets/**",
                "--max-files",
                "3",
                "--human-sample",
                "0.05",
            ],
        )
        .await;
    assert_eq!(scoped["revision"], 1);
    assert_eq!(scoped["scope"]["human_sample_permille"], 50);
    let approved = server.json(&co, &["intent", "approve", &id]).await;
    assert_eq!(approved["approvals"][0]["owns"][0], "gw", "{approved}");

    std::fs::create_dir_all(co.join("gw")).unwrap();
    std::fs::write(co.join("gw/main.rs"), "fn main() {}\n").unwrap();
    server
        .json(&co, &["commit", "-m", "gateway", "--intent", &id])
        .await;
    server.json(&co, &["submit"]).await;
    let explained = server.json(&co, &["intent", "explain"]).await;
    assert_eq!(explained["intent"], id.as_str());
    assert_eq!(explained["in_scope"], true, "{explained}");

    std::fs::write(co.join("README.md"), "outside\n").unwrap();
    server.json(&co, &["commit"]).await;
    let explained = server.json(&co, &["intent", "explain"]).await;
    assert_eq!(explained["in_scope"], false, "{explained}");
    assert!(
        explained["reasons"][0]
            .as_str()
            .unwrap()
            .contains("README.md"),
        "an amend keeps the intent and is judged again: {explained}"
    );
}
