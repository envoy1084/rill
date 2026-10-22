//! Release refs through the API: cut by an owner, landed on only through
//! their lander, never moved by hand.

use std::time::Duration;

use rill_types::Line;

use super::*;

async fn change_on(c: &mut Clients, base: Option<Bytes>, path: &str, body: &str) -> Bytes {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("app"),
            base: (base.map(|b| pb::EditBase {
                base: Some(pb::edit_base::Base::Revision(hex::encode(b))),
            }))
            .into(),
            edits: vec![set_edit(path, body.as_bytes())],
            message: format!("edit {path}"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .change_id
}

#[tokio::test]
async fn a_release_is_cut_and_landed_on_only_through_its_lander() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let base = change_on(&mut c, None, "VERSION", "1.0\n").await;
    land(&mut c, "app", base).await;
    let cut = c
        .repo
        .create_release(pb::CreateReleaseRequest {
            request_id: String::new(),
            parent: common::names::repo("app"),
            release_id: "1.0".into(),
            trunk_position: None,
        })
        .await
        .unwrap()
        .into_owned()
        .r#ref
        .unwrap();
    assert_eq!(
        cut.name,
        format!("{}/refs/release/1.0", common::names::repo("app"))
    );
    let first = c
        .repo
        .get_trunk_position(pb::GetTrunkPositionRequest {
            name: common::names::position("app", 1),
        })
        .await
        .unwrap()
        .into_owned()
        .landing
        .unwrap();
    assert_eq!(cut.target, first.version_hash, "cut at trunk position 1");

    // Trunk moves on; a fix based on the release lands on it alone.
    let next = change_on(&mut c, Some(cut.target.clone()), "VERSION", "2.0\n").await;
    land(&mut c, "app", next).await;
    let fix = change_on(&mut c, Some(cut.target.clone()), "FIX", "fixed\n").await;
    approve(&mut c, &fix).await;
    let ticket = c
        .landing
        .request_land(pb::RequestLandRequest {
            request_id: String::new(),
            change: common::names::change("app", &fix),
            onto: "release/1.0".into(),
        })
        .await
        .unwrap()
        .into_owned()
        .land_request
        .unwrap();
    let status = wait(&mut c, &ticket.name).await;
    assert_eq!(
        status.trunk_position,
        Some(1),
        "the release's first landing"
    );

    let ctx = server.state.repos.resolve("app").await.unwrap();
    let (_, head) = ctx
        .repo
        .line_head(&Line::Release("1.0".into()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        status.landed_hash.as_deref(),
        Some(head.as_bytes().as_slice())
    );
    let release_root = ctx.derive.get::<rill_core::RootTree>(&head).await.unwrap();
    let version = ctx
        .trees
        .lookup(
            release_root,
            &rill_types::RepoPathBuf::new("VERSION").unwrap(),
        )
        .await
        .unwrap();
    assert!(version.is_some());
    assert_eq!(ctx.repo.trunk_head().await.unwrap().unwrap().0.get(), 2);

    let moved = c
        .repo
        .update_ref(pb::UpdateRefRequest {
            request_id: String::new(),
            r#ref: pb::Ref {
                name: cut.name.clone(),
                target: cut.target.clone(),
                ..Default::default()
            }
            .into(),
            etag: head.to_hex(),
            allow_missing: false,
        })
        .await
        .unwrap_err();
    assert_eq!(moved.code, connectrpc::ErrorCode::PermissionDenied);

    let other = change_on(&mut c, Some(cut.target.clone()), "OTHER", "x\n").await;
    approve(&mut c, &other).await;
    let missing = c
        .landing
        .request_land(pb::RequestLandRequest {
            request_id: String::new(),
            change: common::names::change("app", &other),
            onto: "release/9.9".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(missing.code, connectrpc::ErrorCode::NotFound);
}

async fn approve(c: &mut Clients, change_id: &Bytes) {
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("app", change_id),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        c.review
            .vote(pb::VoteRequest {
                request_id: String::new(),
                review: common::names::review("app", change_id),
                label: label.into(),
                value,
                reason: None,
                version: String::new(),
            })
            .await
            .unwrap();
    }
}

async fn wait(c: &mut Clients, name: &str) -> pb::LandRequest {
    for _ in 0..200 {
        let status = c
            .landing
            .get_land_request(pb::GetLandRequestRequest { name: name.into() })
            .await
            .unwrap()
            .into_owned()
            .land_request
            .unwrap();
        match rill_proto::convert::known::<pb::LandStatus>(status.status).unwrap() {
            pb::LandStatus::Landed => return status,
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                panic!("{name} did not land: {:?}", status.error)
            }
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    panic!("{name} did not finish")
}
