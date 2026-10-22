//! The end-to-end scenario and crash-mid-write survival, over a balanced cluster.

use super::balancer::Balancer;
use super::replica::Replica;
use super::retrying::{edit, land_through, op, stored_with_message, write_through};
use crate::*;

async fn cluster_loses_nothing(stores: rill_store::Stores) {
    let mut replicas: Vec<Replica> = (0..3)
        .map(|_| Replica::start(dev_config(), stores.clone()))
        .collect();
    let grpc = Balancer::start(replicas.iter().map(|r| r.grpc).collect()).await;
    let http = Balancer::start(replicas.iter().map(|r| r.http).collect()).await;
    let endpoint = format!("http://{}", grpc.addr);

    crate::end_to_end::scenario(&endpoint, http.addr).await;

    // A server does a write and dies before answering.
    let repo = "main";
    let c = clients_at(&endpoint, None).await;
    let write = edit(repo, "lost-reply.txt", &op());
    // Open the connection first, so the cut falls on the write itself.
    c.repo
        .get_repo(pb::GetRepoRequest { repo: repo.into() })
        .await
        .unwrap();
    grpc.cut_next_request();
    assert!(c.repo.clone().apply_edits(write.clone()).await.is_err());
    let dead = grpc.take_cut().expect("the request reached a server");
    replicas[dead].kill();
    grpc.mark_dead(dead);
    http.mark_dead(dead);

    let change = write_through(&c, write).await;
    assert_eq!(
        stored_with_message(&c, repo, "lost-reply.txt").await,
        1,
        "the retry got the first write's outcome instead of writing again"
    );
    let first = land_through(&c, repo, &change).await;

    // Another server dies between requests; the last one carries on, taking
    // over any lease the dead ones held.
    let second = (0..replicas.len()).find(|i| *i != dead).unwrap();
    replicas[second].kill();
    grpc.mark_dead(second);
    http.mark_dead(second);
    let change = write_through(&c, edit(repo, "survivor.txt", &op())).await;
    assert_eq!(land_through(&c, repo, &change).await, first + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_servers_over_a_shared_store_lose_nothing() {
    cluster_loses_nothing(rill_store::Stores::memory()).await;
}

#[cfg(feature = "fdb")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_servers_over_foundationdb_lose_nothing() {
    let root = format!("rill-test-{}", hex::encode(rill_types::ids::random_id()));
    let cluster = std::env::var("RILL_FDB_CLUSTER_FILE").ok();
    let meta = rill_store::FdbMetaStore::connect(cluster.as_deref(), &root).unwrap();
    let stores = rill_store::Stores::new(
        rill_store::Stores::memory().objects,
        std::sync::Arc::new(meta),
    );
    cluster_loses_nothing(stores).await;
    rill_store::FdbMetaStore::connect(cluster.as_deref(), &root)
        .unwrap()
        .engine()
        .clear_all()
        .await
        .unwrap();
}
