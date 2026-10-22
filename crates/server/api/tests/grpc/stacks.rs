//! Stacks through the review service: a declared stack lands as one
//! ordered group, and landing its bottom alone restacks the rest.

use std::time::Duration;

use super::*;
use crate::common::request_land;

/// A new change on `base` setting `path`; its id and version.
async fn change_on(c: &mut Clients, base: &Bytes, path: &str) -> (Bytes, Bytes) {
    let resp = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            base: Some(pb::EditBase {
                base: Some(pb::edit_base::Base::Root(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(base.clone())),
                })),
            }),
            edits: vec![set_edit(path, path.as_bytes())],
            message: format!("add {path}"),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    (resp.version.unwrap().change_id, resp.hash)
}

async fn approve(c: &mut Clients, change_id: &Bytes) {
    c.review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        c.review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: "app".into(),
                change_id: change_id.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
}

/// A repo with one landed file and a reviewed, declared stack of `n`
/// changes on it; their ids, bottom first.
async fn reviewed_stack(server: &TestServer, c: &mut Clients, n: usize) -> Vec<Bytes> {
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let (root, _) = change_on_empty(c).await;
    land(c, "app", root).await;
    let mut base = trunk_head(server).await;
    let mut ids = Vec::new();
    for i in 0..n {
        let (id, version) = change_on(c, &base, &format!("f{i}")).await;
        approve(c, &id).await;
        ids.push(id);
        base = version;
    }
    c.review
        .set_stack(pb::SetStackRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_ids: ids.clone(),
        })
        .await
        .unwrap();
    ids
}

async fn change_on_empty(c: &mut Clients) -> (Bytes, Bytes) {
    let resp = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            base: None,
            edits: vec![set_edit("README", b"r")],
            message: "base".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    (resp.version.unwrap().change_id, resp.hash)
}

async fn trunk_head(server: &TestServer) -> Bytes {
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let (_, head) = ctx.repo.trunk_head().await.unwrap().unwrap();
    Bytes::copy_from_slice(head.as_bytes())
}

async fn wait_landed(c: &mut Clients, ticket: &str) -> u64 {
    for _ in 0..200 {
        let status = c
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: "app".into(),
                ticket: ticket.into(),
            })
            .await
            .unwrap()
            .into_inner();
        match pb::LandStatus::try_from(status.ticket.as_ref().unwrap().status).unwrap() {
            pb::LandStatus::Landed => return status.seq.unwrap(),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                panic!("{ticket} did not land: {:?}", status.error)
            }
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    panic!("{ticket} did not finish")
}

#[tokio::test]
async fn a_declared_stack_lands_as_one_group() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    let ids = reviewed_stack(&server, &mut c, 3).await;
    let tickets = c
        .review
        .request_land_stack(pb::RequestLandStackRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: ids[2].clone(),
            prefix: false,
        })
        .await
        .unwrap()
        .into_inner()
        .tickets;
    assert_eq!(
        tickets
            .iter()
            .map(|t| t.change_id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    let mut seqs = Vec::new();
    for t in &tickets {
        seqs.push(wait_landed(&mut c, &t.id).await);
    }
    assert_eq!(seqs, vec![2, 3, 4], "in order, on top of each other");
}

#[tokio::test]
async fn landing_the_bottom_alone_restacks_the_rest_onto_it() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    let ids = reviewed_stack(&server, &mut c, 2).await;
    // Trunk moves on, so the bottom lands as a rebase of what the rest
    // was stacked on.
    let head = trunk_head(&server).await;
    let (other, _) = change_on(&mut c, &head, "elsewhere").await;
    land(&mut c, "app", other).await;
    request_land(&mut c, "app", ids[0].clone()).await.unwrap();
    let head = trunk_head(&server).await;
    // The restack follows the land transaction.
    let mut review = None;
    for _ in 0..100 {
        let state = c
            .review
            .get_review(pb::GetReviewRequest {
                repo: "app".into(),
                change_id: ids[1].clone(),
            })
            .await
            .unwrap()
            .into_inner()
            .review
            .unwrap();
        if state.versions.len() > 1 {
            review = Some(state);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let review = review.expect("the change above gets a version on the landed one");
    let current = rill_types::ObjectHash::from_slice(review.versions.last().unwrap()).unwrap();
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let cv = ctx
        .repo
        .get_change_version(&current)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cv.first_parent()
            .map(|p| Bytes::copy_from_slice(p.as_bytes())),
        Some(head)
    );
}
