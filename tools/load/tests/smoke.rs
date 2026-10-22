//! The harness runs end to end against an embedded server without errors.

use std::time::Duration;

use rill_load::{run, Profile, Settings};
use rill_sdk::testing::DevServer;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn short_run_completes_without_errors() {
    let server = DevServer::start().await.unwrap();
    let report = run(Settings {
        endpoint: server.endpoint().to_string(),
        repo: "smoke".into(),
        profile: Profile::of_d1(0.2),
        duration: Duration::from_secs(2),
        max_in_flight: 64,
        hot_share: 0.3,
        disjoint: false,
        token: None,
        watchers: 2,
        report_every: None,
        contended: false,
        burst: None,
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

#[tokio::test(flavor = "multi_thread")]
async fn disjoint_landings_all_land() {
    let server = DevServer::start().await.unwrap();
    let report = run(Settings {
        endpoint: server.endpoint().to_string(),
        repo: "disjoint".into(),
        profile: Profile::of_d1(0.0).with_lands(5.0),
        duration: Duration::from_secs(3),
        max_in_flight: 64,
        hot_share: 0.0,
        disjoint: true,
        token: None,
        watchers: 0,
        report_every: None,
        contended: false,
        burst: None,
    })
    .await
    .unwrap();
    let land = report["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["op"] == "land")
        .cloned()
        .unwrap_or_default();
    assert!(land["ok"].as_u64().unwrap_or(0) > 0, "{report:#}");
    assert_eq!(land["errors"], 0, "{report:#}");
    let ops = report["ops"].as_array().unwrap();
    assert!(
        ops.iter().all(|r| r["errors"] == 0),
        "reads use seeded files: {report:#}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn contended_landings_all_land_through_the_merge_drivers() {
    let mut config = DevServer::config();
    config.verify.enabled = true;
    config.exec.local.workers = 4;
    let server = DevServer::start_with(config).await.unwrap();
    let report = run(Settings {
        endpoint: server.endpoint().to_string(),
        repo: "contended".into(),
        profile: Profile::of_d1(0.0).with_lands(4.0),
        duration: Duration::from_secs(4),
        max_in_flight: 64,
        hot_share: 0.0,
        disjoint: false,
        token: None,
        watchers: 0,
        report_every: None,
        contended: true,
        burst: None,
    })
    .await
    .unwrap();
    let land = report["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["op"] == "land")
        .cloned()
        .unwrap_or_default();
    assert!(land["ok"].as_u64().unwrap_or(0) > 4, "{report:#}");
    assert_eq!(land["errors"], 0, "no give-ups: {report:#}");
}
