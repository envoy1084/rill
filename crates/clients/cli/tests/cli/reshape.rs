//! `rill squash` and `rill split` reshape the working change on the
//! server and leave the checkout's files as they are.

use crate::common::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn squash_folds_into_the_parent_and_split_takes_paths_out() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "shape"]).await;
    let co = home.join("co");
    server
        .json(home, &["checkout", "shape", co.to_str().unwrap()])
        .await;

    std::fs::write(co.join("a.txt"), "a\n").unwrap();
    let lower = server.json(&co, &["new", "-m", "add a"]).await;
    let lower = lower["change_id"].as_str().unwrap().to_string();
    std::fs::write(co.join("b.txt"), "b\n").unwrap();
    let upper = server.json(&co, &["new", "-m", "add b"]).await;
    assert_eq!(upper["stacked_on"], lower);

    let squashed = server.json(&co, &["squash"]).await;
    assert_eq!(squashed["change_id"], lower, "{squashed}");
    let files: Vec<&str> = squashed["version"]["file_changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(files, ["a.txt", "b.txt"], "{squashed}");
    let status = server.json(&co, &["status"]).await;
    assert_eq!(status["working_change"], lower);
    assert_eq!(status["changes"].as_array().unwrap().len(), 2, "{status}");
    assert_eq!(std::fs::read_to_string(co.join("b.txt")).unwrap(), "b\n");

    let split = server
        .json(&co, &["split", "b.txt", "-m", "add b first"])
        .await;
    let below = split["lower"]["change_id"].as_str().unwrap().to_string();
    assert_ne!(below, lower);
    assert_eq!(split["upper"]["change_id"], lower);
    let shown = server.json(&co, &["show", &lower]).await;
    assert_eq!(
        shown["file_changes"].as_array().unwrap().len(),
        1,
        "{shown}"
    );
    let status = server.json(&co, &["status"]).await;
    assert_eq!(
        status["changes"].as_array().unwrap().len(),
        1,
        "only a.txt is above the new base: {status}"
    );
    let log = server.json(&co, &["log"]).await;
    let stack: Vec<&str> = log["stack"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["change_id"].as_str().unwrap())
        .collect();
    assert_eq!(stack, [lower.as_str(), below.as_str()]);
}
