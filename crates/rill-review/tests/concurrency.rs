//! Many reviewers acting on one change at once.

mod common;

use std::sync::Arc;

use futures::future::join_all;
use rill_types::{Principal, ReviewStatus};

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::meta::{KeyRange, MetaStore, Transaction, TxOutcome};
use rill_store::{MemoryMetaStore, MemoryObjectStore, Stores};

use common::{harness_on, LIB_V1};

/// A metadata store with a network's worth of latency on every call, so
/// concurrent requests overlap the way they do against a real backend.
struct Slow(MemoryMetaStore);

const LATENCY: std::time::Duration = std::time::Duration::from_millis(2);

#[async_trait]
impl MetaStore for Slow {
    async fn get(&self, key: &[u8]) -> rill_store::Result<Option<Bytes>> {
        tokio::time::sleep(LATENCY).await;
        self.0.get(key).await
    }

    async fn scan(&self, range: KeyRange) -> rill_store::Result<Vec<(Bytes, Bytes)>> {
        tokio::time::sleep(LATENCY).await;
        self.0.scan(range).await
    }

    async fn transact(&self, tx: Transaction) -> rill_store::Result<TxOutcome> {
        tokio::time::sleep(LATENCY).await;
        self.0.transact(tx).await
    }

    async fn counter(&self, key: &[u8]) -> rill_store::Result<i64> {
        self.0.counter(key).await
    }
}

fn slow_stores() -> Stores {
    Stores::new(
        Arc::new(MemoryObjectStore::new()),
        Arc::new(Slow(MemoryMetaStore::new())),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reviewers_voting_at_once_never_exhaust_retries() {
    let h = Arc::new(harness_on(slow_stores()).await);
    let root = h.root().await;
    let (cid, _) = h.child(&h.alice, root, &[("src/lib.rs", LIB_V1)]).await;
    h.service.submit(&cid, &h.alice).await.unwrap();
    let tasks = (0..24).map(|i| {
        let h = h.clone();
        let agent = Principal::agent(format!("agent-{i}"));
        tokio::spawn(async move { h.service.vote(&cid, &agent, "Review", 1, None).await })
    });
    for result in join_all(tasks).await {
        result.unwrap().unwrap();
    }
    assert_eq!(
        h.service.get(&cid).await.unwrap().current_votes().count(),
        24
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn votes_cast_together_reach_the_status_they_add_up_to() {
    let h = Arc::new(harness_on(slow_stores()).await);
    let root = h.root().await;
    for round in 0..10 {
        let (cid, _) = h
            .child(&h.alice, root, &[(&format!("src/f{round}.rs"), "x\n")])
            .await;
        h.service.submit(&cid, &h.alice).await.unwrap();
        // Two agent approvals and a human owner +1 together make the
        // change landable; no two of them do.
        let votes = [
            (h.agent_a.clone(), "Review", 1),
            (h.agent_b.clone(), "Review", 1),
            (h.bob.clone(), "Owner", 1),
        ];
        let tasks = votes.into_iter().map(|(who, label, value)| {
            let h = h.clone();
            tokio::spawn(async move { h.service.vote(&cid, &who, label, value, None).await })
        });
        for result in join_all(tasks).await {
            result.unwrap().unwrap();
        }
        assert_eq!(
            h.service.get(&cid).await.unwrap().status,
            ReviewStatus::Landable,
            "round {round}"
        );
    }
}
