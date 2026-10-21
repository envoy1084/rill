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
        token: None,
        watchers: 2,
        report_every: None,
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
    let ledger = &report["ledger"];
    assert!(
        ledger["acknowledged_versions"].as_u64().unwrap() > 0,
        "{report:#}"
    );
    assert_eq!(
        ledger["lost_versions"].as_array().unwrap().len(),
        0,
        "{report:#}"
    );
}
