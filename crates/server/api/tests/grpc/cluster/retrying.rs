//! Client calls that retry through server deaths with the same operation id.

use std::future::Future;

use crate::common::names;
use crate::*;

const ATTEMPTS: usize = 100;

/// Call until the cluster answers, retrying transport failures (a dead
/// server, a cut connection) the way a client with an operation id may.
pub(super) async fn retry<V, F, Fut>(what: &str, mut call: F) -> V::Owned
where
    F: FnMut() -> Fut,
    Fut: Future<
        Output = Result<connectrpc::client::UnaryResponse<buffa::view::OwnedView<V>>, ConnectError>,
    >,
    V: buffa::MessageView<'static>,
{
    for _ in 0..ATTEMPTS {
        match call().await {
            Ok(response) => return response.into_owned(),
            Err(s) if matches!(s.code, Code::Unavailable | Code::Unknown | Code::Canceled) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(s) => panic!("{what}: {s}"),
        }
    }
    panic!("{what}: the cluster never answered");
}

pub(super) fn op() -> String {
    names::request_id()
}

pub(super) fn edit(repo: &str, path: &str, op: &str) -> pb::ApplyEditsRequest {
    pb::ApplyEditsRequest {
        request_id: op.into(),
        parent: names::repo(repo),
        base: common::trunk_base().into(),
        edits: vec![set_edit(path, path.as_bytes())],
        message: path.into(),
        intent: String::new(),
        provenance: None.into(),
        change: String::new(),
    }
}

/// Review and land `change`, retrying every step; returns its position.
pub(super) async fn land_through(c: &Clients, repo: &str, change: &Bytes) -> u64 {
    let submit = pb::SubmitChangeRequest {
        request_id: op(),
        change: names::change(repo, change),
    };
    retry("submit", || {
        let (svc, request) = (c.review.clone(), submit.clone());
        async move { svc.submit_change(request).await }
    })
    .await;
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        let vote = pb::VoteRequest {
            request_id: op(),
            review: names::review(repo, change),
            label: label.into(),
            value,
            reason: None,
            version: String::new(),
        };
        retry("vote", || {
            let (svc, request) = (c.review.clone(), vote.clone());
            async move { svc.vote(request).await }
        })
        .await;
    }
    let land = pb::RequestLandRequest {
        request_id: op(),
        change: names::change(repo, change),
        onto: String::new(),
    };
    let ticket = retry("request land", || {
        let (svc, request) = (c.landing.clone(), land.clone());
        async move { svc.request_land(request).await }
    })
    .await
    .land_request
    .unwrap();
    let status = pb::GetLandRequestRequest {
        name: ticket.name.clone(),
    };
    // Long enough for a survivor to take over a dead server's lease.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let current = retry("land status", || {
            let (svc, request) = (c.landing.clone(), status.clone());
            async move { svc.get_land_request(request).await }
        })
        .await
        .land_request
        .unwrap();
        match rill_proto::convert::known::<pb::LandStatus>(current.status).unwrap() {
            pb::LandStatus::Landed => return current.trunk_position.unwrap(),
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
pub(super) async fn stored_with_message(c: &Clients, repo: &str, message: &str) -> usize {
    let mut log = c
        .repo
        .clone()
        .watch_versions(pb::WatchVersionsRequest {
            parent: names::repo(repo),
            after: Some(Bytes::new()),
        })
        .await
        .unwrap()
        .messages();
    let mut count = 0;
    while let Ok(Some(entry)) = tokio::time::timeout(Duration::from_millis(500), log.next())
        .await
        .map(|m| m.transpose().unwrap())
    {
        count += usize::from(entry.version.unwrap().message == message);
    }
    count
}

/// Apply `request`, retrying with its operation id; returns the change id.
pub(super) async fn write_through(c: &Clients, request: pb::ApplyEditsRequest) -> Bytes {
    retry("write", || {
        let (svc, request) = (c.changes.clone(), request.clone());
        async move { svc.apply_edits(request).await }
    })
    .await
    .version
    .unwrap()
    .change_id
}
