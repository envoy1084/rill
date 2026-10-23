//! `rill doctor` reports the server, the daemon, the cache tiers and the
//! checkouts.

mod common;

use common::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn doctor_reports_the_server_daemon_and_checkouts() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    server.json(home, &["repo", "create", "doc"]).await;
    let co = home.join("co");
    server
        .json(home, &["checkout", "doc", co.to_str().unwrap()])
        .await;
    std::fs::write(co.join("a.txt"), "a\n").unwrap();
    server.json(&co, &["status"]).await;

    let report = server.json(&co, &["doctor"]).await;
    assert_eq!(report["server"]["reachable"], true, "{report}");
    assert_eq!(report["server"]["principal"], "dev");
    assert_eq!(report["daemon"]["running"], true);
    let checkouts = report["checkouts"].as_array().unwrap();
    assert_eq!(checkouts.len(), 1);
    assert_eq!(checkouts[0]["view"], "eager");
    assert_eq!(checkouts[0]["pending"], 1);
    assert!(report["tiers"]["local"]["reads"].is_u64());
    assert!(report["warnings"].is_array());

    let human = server.ok(&co, &["doctor"]).await;
    assert!(human.contains("checkouts"), "{human}");
    assert!(human.contains("eager"), "{human}");
}
