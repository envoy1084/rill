//! The everyday commands added on top of the core set: clone, commit,
//! undo and redo, comment and reply, inbox, land cancel and bug-report.

use crate::common::{approve, Server};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clone_commit_redo_comment_inbox_cancel_and_bug_report() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "everyday"]).await;

    let cloned = server.json(home, &["clone", "everyday", "--eager"]).await;
    let co = home.join("everyday");
    assert_eq!(
        cloned["dir"],
        std::fs::canonicalize(&co).unwrap().display().to_string()
    );

    std::fs::write(co.join("a.txt"), "a\n").unwrap();
    let first = server.json(&co, &["commit", "-m", "first"]).await;
    let change = first["change_id"].as_str().unwrap().to_string();
    std::fs::write(co.join("b.txt"), "b\n").unwrap();
    let second = server.json(&co, &["commit"]).await;
    assert_eq!(
        second["change_id"], change,
        "commit amends the working change"
    );
    assert_eq!(
        second["version"]["message"], "first",
        "and keeps its message"
    );

    let (code, out) = server.rill(&co, &["redo"]).await;
    assert_eq!(code, 2, "nothing to redo yet: {out}");

    server.json(&co, &["submit"]).await;
    let opened = server
        .json(
            &co,
            &["comment", &change, "-m", "looks fine", "--path", "a.txt"],
        )
        .await;
    let thread = opened["thread_id"].as_str().unwrap();
    let replied = server
        .json(&co, &["review", "reply", &change, thread, "-m", "thanks"])
        .await;
    assert_eq!(replied["comments"][1]["body"], "thanks", "{replied}");
    let inbox = server.json(&co, &["inbox"]).await;
    assert_eq!(inbox["yours"][0]["change"], change, "{inbox}");

    approve(&server, &co, &change).await;
    // Landing syncs the checkout, which moves the server workspace; undo
    // reverts that move and redo makes it again.
    server.json(&co, &["land", "--wait"]).await;
    server.json(&co, &["undo"]).await;
    server.json(&co, &["redo"]).await;

    std::fs::write(co.join("c.txt"), "c\n").unwrap();
    let held = server.json(&co, &["new", "-m", "held"]).await;
    let held = held["change_id"].as_str().unwrap().to_string();
    server.json(&co, &["submit"]).await;
    approve(&server, &co, &held).await;
    server
        .json(&co, &["trunk", "freeze", "--reason", "hold"])
        .await;
    server.json(&co, &["land"]).await;
    let cancelled = server.json(&co, &["land", "cancel", "-m", "not yet"]).await;
    assert_eq!(cancelled["status"], "cancelled", "{cancelled}");

    let out = co.join("report.md");
    let report = server
        .json(&co, &["bug-report", "-o", out.to_str().unwrap()])
        .await;
    let text = std::fs::read_to_string(report["path"].as_str().unwrap()).unwrap();
    assert!(text.contains("# rill bug report") && text.contains("rill doctor"));
}
