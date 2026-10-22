//! Release refs through the API: cut by an owner, landed on only through
//! their lander, never moved by hand.

use std::time::Duration;

use rill_types::Line;

use super::*;

async fn change_on(c: &mut Clients, base: Option<Bytes>, path: &str, body: &str) -> Bytes {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            base: (base.map(|b| pb::EditBase {
                base: Some(pb::edit_base::Base::Root(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(b)),
                })),
            }))
            .into(),
            edits: vec![set_edit(path, body.as_bytes())],
            message: format!("edit {path}"),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap()
        .change_id
}

#[tokio::test]
async fn a_release_is_cut_and_landed_on_only_through_its_lander() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let base = change_on(&mut c, None, "VERSION", "1.0\n").await;
    land(&mut c, "app", base).await;
    let cut = c
        .repo
        .cut_release(pb::CutReleaseRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            name: "1.0".into(),
            at: None,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!((cut.r#ref.as_str(), cut.trunk_seq), ("release/1.0", 1));

    // Trunk moves on; a fix based on the release lands on it alone.
    let next = change_on(&mut c, Some(cut.hash.clone()), "VERSION", "2.0\n").await;
    land(&mut c, "app", next).await;
    let fix = change_on(&mut c, Some(cut.hash.clone()), "FIX", "fixed\n").await;
    approve(&mut c, &fix).await;
    let ticket = c
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: fix,
            onto: Some("release/1.0".into()),
        })
        .await
        .unwrap()
        .into_owned()
        .ticket
        .unwrap();
    let status = wait(&mut c, &ticket.id).await;
    assert_eq!(status.seq, Some(1), "the release's first landing");

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
        .move_ref(pb::MoveRefRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            update: (pb::RefUpdate {
                name: "release/1.0".into(),
                expected: Some(Bytes::copy_from_slice(head.as_bytes())),
                target: Some(cut.hash.clone()),
            })
            .into(),
        })
        .await
        .unwrap_err();
    assert_eq!(moved.code, connectrpc::ErrorCode::PermissionDenied);

    let other = change_on(&mut c, Some(cut.hash.clone()), "OTHER", "x\n").await;
    approve(&mut c, &other).await;
    let missing = c
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: other,
            onto: Some("release/9.9".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(missing.code, connectrpc::ErrorCode::NotFound);
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

async fn wait(c: &mut Clients, ticket: &str) -> pb::GetLandStatusResponse {
    for _ in 0..200 {
        let status = c
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: "app".into(),
                ticket: ticket.into(),
            })
            .await
            .unwrap()
            .into_owned();
        match rill_proto::convert::known::<pb::LandStatus>(
            status.ticket.as_option().unwrap().status,
        )
        .unwrap()
        {
            pb::LandStatus::Landed => return status,
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                panic!("{ticket} did not land: {:?}", status.error)
            }
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    panic!("{ticket} did not finish")
}
