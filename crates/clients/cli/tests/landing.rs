//! Landing from a checkout: outcomes reported to the author and what the
//! checkout looks like afterwards.

mod common;

use common::{approve, Server};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stale_landing_reports_conflict_and_recovers() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "l"]).await;
    let alice = home.join("alice");
    server
        .json(home, &["checkout", "l", alice.to_str().unwrap()])
        .await;
    std::fs::write(alice.join("f.txt"), "one\ntwo\nthree\n").unwrap();
    let base = server.json(&alice, &["new", "-m", "base"]).await;
    let base_id = base["change_id"].as_str().unwrap().to_string();
    server.json(&alice, &["submit"]).await;
    approve(&server, &alice, &base_id).await;

    // Landing moves the author's checkout onto trunk.
    let landed = server.json(&alice, &["land", "--wait"]).await;
    assert_eq!(landed["synced"], true, "{landed}");
    let shown = server.json(&alice, &["show"]).await;
    assert_eq!(shown["info"]["trunk_seq"], 1, "{shown}");

    let bob = home.join("bob");
    server
        .json(home, &["checkout", "l", bob.to_str().unwrap()])
        .await;
    std::fs::write(bob.join("f.txt"), "one\nTWO-bob\nthree\n").unwrap();
    let theirs = server.json(&bob, &["new", "-m", "bob"]).await;
    server.json(&bob, &["submit"]).await;
    approve(&server, &bob, theirs["change_id"].as_str().unwrap()).await;
    server.json(&bob, &["land", "--wait"]).await;

    // Alice edits the same line without syncing; the lander's rebase
    // conflicts and she must be told so, not that her ticket expired.
    std::fs::write(alice.join("f.txt"), "one\nTWO-alice\nthree\n").unwrap();
    let ours = server.json(&alice, &["new", "-m", "alice"]).await;
    let ours_id = ours["change_id"].as_str().unwrap().to_string();
    server.json(&alice, &["submit"]).await;
    approve(&server, &alice, &ours_id).await;
    let (code, out) = server.rill(&alice, &["--json", "land", "--wait"]).await;
    assert_eq!(code, 3, "{out}");
    assert!(
        out.contains("rebase onto trunk conflicts at: f.txt"),
        "{out}"
    );

    // Markers left in the file are refused; a real resolution lands.
    let (code, _) = server.rill(&alice, &["sync"]).await;
    assert_eq!(code, 3);
    let (code, out) = server.rill(&alice, &["resolve", "f.txt"]).await;
    assert_ne!(code, 0, "{out}");
    std::fs::write(alice.join("f.txt"), "one\nTWO-both\nthree\n").unwrap();
    server.json(&alice, &["resolve", "f.txt"]).await;
    // The change is still recorded on the old base until amended; landing
    // or submitting now would land the stale, conflicting version.
    for cmd in [&["land", "--wait"][..], &["submit"][..]] {
        let (code, out) = server.rill(&alice, cmd).await;
        assert_eq!(code, 3, "{cmd:?}: {out}");
    }
    let queue = server.json(&alice, &["queue"]).await;
    assert_eq!(
        queue["land_requests"],
        serde_json::json!([]),
        "nothing was sent to land: {queue}"
    );
    server.json(&alice, &["amend"]).await;
    approve(&server, &alice, &ours_id).await;
    let landed = server.json(&alice, &["land", "--wait"]).await;
    assert_eq!(landed["seq"], 3);
    assert_eq!(landed["synced"], true);
}
