//! The long-running lander: shutdown, leases and durable requests.

use std::sync::Arc;
use std::time::Duration;

use rill_lander::{AlwaysGreen, LandStatus, NoopHooks};
use rill_types::{RiskClass, TrunkSeq};

use crate::common::{fast_policy, request, Fixture};

#[tokio::test]
async fn run_forever_processes_until_shutdown() {
    let fx = Fixture::new().await;
    let lander = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let shutdown = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(lander.clone().run_forever(shutdown.clone()));

    let (_, cv) = fx.commit(None, &[("a", "a")], "a").await;
    let ticket = lander
        .enqueue(request(&cv, RiskClass::Trivial))
        .await
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = lander.queue().status(ticket).await.unwrap().unwrap();
            if status.is_terminal() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(status, LandStatus::Landed { seq: TrunkSeq(1) });
    shutdown.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn a_standby_lander_waits_for_the_lease() {
    let fx = Fixture::new().await;
    let active = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let standby = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    assert!(active.lease().await.unwrap().is_some());

    let (_, a) = fx.commit(Some(base), &[("a.txt", "a")], "a").await;
    let ticket = standby
        .enqueue(request(&a, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(
        standby.run_once().await.unwrap(),
        0,
        "standby leaves it queued"
    );
    assert_eq!(fx.head().await.0, TrunkSeq(1));

    active.release().await;
    assert_eq!(standby.run_once().await.unwrap(), 1);
    assert!(matches!(
        standby.queue().status(ticket).await.unwrap().unwrap(),
        LandStatus::Landed { seq: TrunkSeq(2) }
    ));
}

#[tokio::test]
async fn a_request_outlives_the_lander_that_accepted_it() {
    let fx = Fixture::new().await;
    let (base, _) = fx.commit(None, &[("base", "0")], "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    let (_, a) = fx.commit(Some(base), &[("a.txt", "a")], "a").await;

    let crashed = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    assert!(crashed.lease().await.unwrap().is_some());
    let ticket = crashed
        .enqueue(request(&a, RiskClass::Trivial))
        .await
        .unwrap();
    assert_eq!(
        crashed
            .enqueue(request(&a, RiskClass::Trivial))
            .await
            .unwrap(),
        ticket,
        "asking again returns the same request"
    );
    // The process goes away before serving the request; its successor
    // takes the lease and finds the request in the store.
    crashed.release().await;
    drop(crashed);

    let successor = fx.lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy());
    assert_eq!(successor.run_once().await.unwrap(), 1);
    assert_eq!(
        successor.queue().status(ticket).await.unwrap(),
        Some(LandStatus::Landed { seq: TrunkSeq(2) })
    );
    assert!(successor.queue().is_empty().await.unwrap());
}
