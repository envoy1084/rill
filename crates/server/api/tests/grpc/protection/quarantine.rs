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
    let version = hash_of(&held);
    let change = held.version.unwrap().change_id;
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("app", &change),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        c.review
            .vote(pb::VoteRequest {
                review: common::names::review("app", &change),
                label: label.into(),
                value,
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let ticket = c
        .landing
        .request_land(pb::RequestLandRequest {
            request_id: String::new(),
            change: common::names::change("app", &change),
            onto: String::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .land_request
        .unwrap();
    let mut refused = None;
    for _ in 0..100 {
        let status = c
            .landing
            .get_land_request(pb::GetLandRequestRequest {
                name: ticket.name.clone(),
            })
            .await
            .unwrap()
            .into_owned()
            .land_request
            .unwrap();
        match rill_proto::convert::known::<pb::LandStatus>(status.status).unwrap() {
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
        .import_trunk_position(pb::ImportTrunkPositionRequest {
            parent: common::names::repo("app"),
            expected: head,
            version: version.clone(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(appended.code, Code::FailedPrecondition, "{appended:?}");

    c.protection
        .release_version(pb::ReleaseVersionRequest {
            request_id: String::new(),
            version: common::names::version("app", &version),
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
        .quarantined(&ObjectHash::from_slice(&version).unwrap())
        .await
        .unwrap()
        .is_none());
    server.stop().await;
}
