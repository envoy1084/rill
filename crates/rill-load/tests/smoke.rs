//! The harness runs end to end against an embedded server without errors.

use std::time::Duration;

use rill_load::{run, EmbeddedServer, Profile, Settings};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn short_run_completes_without_errors() {
    let server = EmbeddedServer::start().await.unwrap();
    let report = run(Settings {
        endpoint: server.endpoint.clone(),
        repo: "smoke".into(),
        profile: Profile::of_d1(0.2),
        duration: Duration::from_secs(2),
        max_in_flight: 64,
        hot_share: 0.3,
    })
    .await
    .unwrap();
    let ops = report["ops"].as_array().unwrap();
    assert!(ops.iter().all(|r| r["errors"] == 0), "{report:#}");
    let reads: u64 = ops
        .iter()
        .filter(|r| ["read_file", "list_dir", "grep", "log"].contains(&r["op"].as_str().unwrap()))
        .map(|r| r["ok"].as_u64().unwrap())
        .sum();
    assert!(reads > 50, "{report:#}");
}
