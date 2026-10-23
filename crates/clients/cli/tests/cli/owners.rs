//! `rill owners`: the owner sets covering a path and a change's standing
//! with them, after an `OWNERS.toml` lands.

use crate::common::{approve, Server};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owners_show_and_explain_follow_trunk_owners_files() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "own"]).await;
    let co = home.join("co");
    server
        .json(home, &["checkout", "own", co.to_str().unwrap()])
        .await;

    std::fs::create_dir_all(co.join("pay")).unwrap();
    std::fs::write(
        co.join("pay/OWNERS.toml"),
        "owners = [\"user:ann\"]\n[[rule]]\npaths = [\"*.sql\"]\nowners = [\"team:dba\"]\nrequired = 2\n",
    )
    .unwrap();
    let owners = server.json(&co, &["new", "-m", "pay owners"]).await;
    let owners = owners["change_id"].as_str().unwrap().to_string();
    server.json(&co, &["submit"]).await;
    approve(&server, &co, &owners).await;
    server
        .json(&co, &["review", "vote", &owners, "Owner", "+1"])
        .await;
    server.json(&co, &["land", "--wait"]).await;

    let shown = server
        .json(&co, &["owners", "show", "pay/schema.sql"])
        .await;
    let sets = shown["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 1, "{shown}");
    assert_eq!(sets[0]["id"], "pay#rule0");
    assert_eq!(sets[0]["required"], 2);
    let shown = server.json(&co, &["owners", "show", "pay/api.rs"]).await;
    assert_eq!(shown["sets"][0]["owners"][0], "user:ann");

    std::fs::write(co.join("pay/api.rs"), "fn pay() {}\n").unwrap();
    let change = server.json(&co, &["new", "-m", "pay api"]).await;
    let change = change["change_id"].as_str().unwrap().to_string();
    server.json(&co, &["submit"]).await;
    approve(&server, &co, &change).await;
    let explained = server.json(&co, &["owners", "explain"]).await;
    assert_eq!(explained["covered"], false, "{explained}");
    assert_eq!(explained["sets"][0]["set"]["id"], "pay#default");
    assert_eq!(explained["sets"][0]["approvals"], 0);
    let (code, out) = server.rill(&co, &["--json", "review", "evaluate"]).await;
    assert_ne!(code, 0, "a denied landing is a failure: {out}");
    let decision: serde_json::Value = serde_json::from_str(&out).unwrap();
    let decision = &decision["result"];
    assert_eq!(decision["allowed"], false, "{decision}");
    assert!(
        decision["matched_policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "block_uncovered_owners"),
        "{decision}"
    );
}
