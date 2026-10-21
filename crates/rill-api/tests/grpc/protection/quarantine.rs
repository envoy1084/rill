//! Quarantined versions reach trunk only after an owner releases them.

use rill_types::{ObjectHash, Principal};

use super::super::*;
use super::fixtures::*;

#[tokio::test(flavor = "multi_thread")]
async fn quarantined_versions_wait_for_an_owner() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    let rules = apply(
        &mut c,
        "app",
        vec![set_edit(".rill/secrets.toml", b"mode = \"quarantine\"\n")],
    )
    .await
    .unwrap();
    let rules_change = rules.version.unwrap().change_id;
    land_with(&mut c, "app", rules_change, &[("Owner", 1)]).await;

    let held = apply(
        &mut c,
        "app",
        vec![set_edit(
            "deploy.env",
            format!("AWS={AWS_KEY}\n").as_bytes(),
        )],
    )
    .await
    .expect("quarantine stores the version");
    let version = held.hash.clone();
    let change = held.version.unwrap().change_id;
    c.review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: change.clone(),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        c.review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: "app".into(),
                change_id: change.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
    let ticket = c
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: change.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .ticket
        .unwrap();
    let mut refused = None;
    for _ in 0..100 {
        let status = c
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: "app".into(),
                ticket: ticket.id.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        match pb::LandStatus::try_from(status.ticket.as_ref().unwrap().status).unwrap() {
            pb::LandStatus::Landed => panic!("a quarantined version landed"),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                refused = status.error;
                break;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    assert!(refused.expect("refused").contains("quarantined"));

    // Nor can an importer append it to trunk.
    let head = server
        .state
        .repos
        .resolve("app")
        .await
        .unwrap()
        .repo
        .trunk_head()
        .await
        .unwrap()
        .map(|(_, h)| Bytes::copy_from_slice(h.as_bytes()));
    let appended = c
        .admin
        .append_trunk(pb::AppendTrunkRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            expected: head,
            version: version.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(appended.code(), Code::FailedPrecondition, "{appended:?}");

    c.protection
        .clear_quarantine(pb::ClearQuarantineRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            version,
        })
        .await
        .unwrap();
    assert!(server
        .state
        .repos
        .resolve("app")
        .await
        .unwrap()
        .repo
        .quarantined(&ObjectHash::from_slice(&held.hash).unwrap())
        .await
        .unwrap()
        .is_none());
    server.stop().await;
}
