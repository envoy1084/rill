//! Three servers behind a load balancer serve the end-to-end scenario, and
//! killing any of them, even between doing a write and answering it, loses
//! nothing: clients retry with the same operation id and get one outcome.

mod balancer;
mod replica;

use std::future::Future;

use balancer::Balancer;
use replica::Replica;

use super::*;

const ATTEMPTS: usize = 100;

/// Call until the cluster answers, retrying transport failures (a dead
/// server, a cut connection) the way a client with an operation id may.
async fn retry<T, F, Fut>(what: &str, mut call: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<tonic::Response<T>, Status>>,
{
    for _ in 0..ATTEMPTS {
        match call().await {
            Ok(response) => return response.into_inner(),
            Err(s)
                if matches!(
                    s.code(),
                    Code::Unavailable | Code::Unknown | Code::Cancelled
                ) =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(s) => panic!("{what}: {s}"),
        }
    }
    panic!("{what}: the cluster never answered");
}

fn op() -> Bytes {
    Bytes::copy_from_slice(&rill_types::ids::random_id())
}

fn edit(repo: &str, path: &str, op: &Bytes) -> pb::ApplyEditsRequest {
    pb::ApplyEditsRequest {
        operation_id: op.clone(),
        repo: repo.into(),
        base: Some(pb::EditBase {
            base: Some(pb::edit_base::Base::Root(trunk())),
        }),
        edits: vec![set_edit(path, path.as_bytes())],
        message: path.into(),
        intent: None,
        provenance: None,
        change_id: None,
    }
}

/// Review and land `change`, retrying every step; returns its position.
async fn land_through(c: &Clients, repo: &str, change: &Bytes) -> u64 {
    let submit = pb::SubmitForReviewRequest {
        operation_id: op(),
        repo: repo.into(),
        change_id: change.clone(),
    };
    retry("submit", || {
        let (mut svc, request) = (c.review.clone(), submit.clone());
        async move { svc.submit_for_review(request).await }
    })
    .await;
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        let vote = pb::VoteRequest {
            operation_id: op(),
            repo: repo.into(),
            change_id: change.clone(),
            label: label.into(),
            value,
            reason: None,
            version: None,
        };
        retry("vote", || {
            let (mut svc, request) = (c.review.clone(), vote.clone());
            async move { svc.vote(request).await }
        })
        .await;
    }
    let land = pb::RequestLandRequest {
        operation_id: op(),
        repo: repo.into(),
        change_id: change.clone(),
    };
    let ticket = retry("request land", || {
        let (mut svc, request) = (c.review.clone(), land.clone());
        async move { svc.request_land(request).await }
    })
    .await
    .ticket
    .unwrap();
    let status = pb::GetLandStatusRequest {
        repo: repo.into(),
        ticket: ticket.id.clone(),
    };
    // Long enough for a survivor to take over a dead server's lease.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let current = retry("land status", || {
            let (mut svc, request) = (c.review.clone(), status.clone());
            async move { svc.get_land_status(request).await }
        })
        .await;
        match pb::LandStatus::try_from(current.ticket.as_ref().unwrap().status).unwrap() {
            pb::LandStatus::Landed => return current.seq.unwrap(),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                panic!("landing failed: {:?}", current.error)
            }
            _ => {}
        }
        assert!(tokio::time::Instant::now() < deadline, "never landed");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// How many versions in the repository's version log carry `message`.
async fn stored_with_message(c: &Clients, repo: &str, message: &str) -> usize {
    let mut log = c
        .repo
        .clone()
        .watch_log(pb::WatchLogRequest {
            repo: repo.into(),
            after: Some(Bytes::new()),
        })
        .await
        .unwrap()
        .into_inner();
    let mut count = 0;
    while let Ok(Some(entry)) = tokio::time::timeout(Duration::from_millis(500), log.message())
        .await
        .map(|m| m.unwrap())
    {
        count += usize::from(entry.info.unwrap().message == message);
    }
    count
}

/// Apply `request`, retrying with its operation id; returns the change id.
async fn write_through(c: &Clients, request: pb::ApplyEditsRequest) -> Bytes {
    retry("write", || {
        let (mut svc, request) = (c.repo.clone(), request.clone());
        async move { svc.apply_edits(request).await }
    })
    .await
    .version
    .unwrap()
    .change_id
}

async fn cluster_loses_nothing(stores: rill_store::Stores) {
    let mut replicas: Vec<Replica> = (0..3)
        .map(|_| Replica::start(dev_config(), stores.clone()))
        .collect();
    let grpc = Balancer::start(replicas.iter().map(|r| r.grpc).collect()).await;
    let http = Balancer::start(replicas.iter().map(|r| r.http).collect()).await;
    let endpoint = format!("http://{}", grpc.addr);

    super::end_to_end::scenario(&endpoint, http.addr).await;

    // A server does a write and dies before answering.
    let repo = "main";
    let c = clients_at(&endpoint, None).await;
    let write = edit(repo, "lost-reply.txt", &op());
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
