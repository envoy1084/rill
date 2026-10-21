//! Stopping in order: unready while draining, then stopped within the
//! deadline with leases given up for a standby.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_drains_then_stops_and_frees_its_leases() {
    let stores = rill_store::Stores::memory();
    let config = ServerConfig {
        shutdown: rill_api::ShutdownConfig {
            drain_secs: 1,
            grace_secs: 10,
        },
        ..dev_config()
    };
    let signal = CancellationToken::new();
    let state = AppState::with_stores(config, stores.clone(), CancellationToken::new()).unwrap();
    let listeners = Listeners::bind(
        "127.0.0.1:0".parse().unwrap(),
        "127.0.0.1:0".parse().unwrap(),
    )
    .await
    .unwrap();
    let (_, http) = listeners.addrs().unwrap();
    let running = tokio::spawn(rill_api::serve_until(
        state.clone(),
        listeners,
        signal.clone(),
    ));

    let repo = state
        .repos
        .create(
            "drain",
            "",
            Default::default(),
            &rill_types::Principal::human("a"),
        )
        .await
        .unwrap();
    let store = repo.repo.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while store.lease_epoch("lander").await.unwrap() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the lander never took its lease"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(http_get(http, "/readyz").await.0, 200);

    signal.cancel();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, _, body) = http_get(http, "/readyz").await;
    assert_eq!((status, body.as_slice()), (503, b"draining".as_slice()));

    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .expect("stopped within the deadline")
        .unwrap()
        .unwrap();
    assert!(
        store
            .try_acquire("lander", "standby")
            .await
            .unwrap()
            .is_some(),
        "a standby takes over at once"
    );
}
