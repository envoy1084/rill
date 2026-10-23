//! Review mechanics: a suggested edit applied as a new version, the
//! interdiff between versions, threads that go outdated or follow a
//! rename, reopening, and the landed version closing the review.

use super::super::*;
use super::fixture::*;
use super::writes::{change_of, vote};

async fn amend(c: &mut Clients, change: &Bytes, edits: Vec<pb::Edit>) -> Bytes {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("acl"),
            change: common::names::change("acl", change),
            edits,
            message: "m".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash
}

async fn review(c: &mut Clients, change: &Bytes) -> pb::ReviewState {
    c.review
        .get_review(pb::GetReviewRequest {
            name: common::names::review("acl", change),
        })
        .await
        .unwrap()
        .into_owned()
        .review
        .unwrap()
}

fn lines(version: &Bytes, path: &str, start: u32, end: u32) -> pb::ThreadAnchor {
    pb::ThreadAnchor {
        anchor: Some(pb::thread_anchor::Anchor::Lines(pb::thread_anchor::Lines {
            version: version.clone(),
            path: path.into(),
            start,
            end,
            context_hash: String::new(),
        })),
    }
}

async fn thread(
    c: &mut Clients,
    change: &Bytes,
    anchor: pb::ThreadAnchor,
    suggestion: Option<&str>,
) -> pb::Thread {
    c.review
        .create_thread(pb::CreateThreadRequest {
            parent: common::names::review("acl", change),
            anchor: Some(anchor).into(),
            body: "look here".into(),
            suggestion: suggestion.map(str::to_string),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .thread
        .unwrap()
}

#[tokio::test]
async fn suggestions_interdiffs_anchors_and_the_landed_version() {
    let mut acl = Acl::start().await;
    acl.grant("pub", "writer", &bob()).await;
    let mut bob_c = acl.as_principal(&bob()).await;
    let v1 = super::writes::edit(&mut bob_c, None, vec![set_edit("pub/m.rs", b"a\nb\nc\n")])
        .await
        .unwrap();
    let change = change_of(&mut bob_c, &v1).await;
    bob_c
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();

    let t = thread(
        &mut acl.owner,
        &change,
        lines(&v1, "pub/m.rs", 2, 2),
        Some("B"),
    )
    .await;
    let carol = rill_types::Principal::human("carol");
    acl.grant("pub", "writer", &carol).await;
    assert_eq!(
        acl.as_principal(&carol)
            .await
            .review
            .apply_suggestion(pb::ApplySuggestionRequest {
                thread: t.name.clone(),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code,
        Code::PermissionDenied,
        "only the change's owner (or an admin) applies"
    );
    let v2 = bob_c
        .review
        .apply_suggestion(pb::ApplySuggestionRequest {
            thread: t.name.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash;
    let state = review(&mut acl.owner, &change).await;
    assert_eq!(state.versions.last(), Some(&v2));
    assert!(
        state
            .threads
            .iter()
            .find(|x| x.id == t.id)
            .unwrap()
            .resolved
    );

    let diff = acl
        .owner
        .review
        .get_interdiff(pb::GetInterdiffRequest {
            change: common::names::change("acl", &change),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(diff.files.len(), 1);
    assert!(
        diff.files[0].unified.contains("-b\n+B\n"),
        "{}",
        diff.files[0].unified
    );

    // A comment whose lines disappear goes outdated.
    let gone = thread(&mut acl.owner, &change, lines(&v2, "pub/m.rs", 2, 2), None).await;
    let v3 = amend(
        &mut bob_c,
        &change,
        vec![set_edit("pub/m.rs", b"x\ny\nz\nw\n")],
    )
    .await;
    let state = review(&mut acl.owner, &change).await;
    assert!(
        state
            .threads
            .iter()
            .find(|x| x.id == gone.id)
            .unwrap()
            .outdated
    );

    // A file comment follows the file's rename.
    let file = thread(
        &mut acl.owner,
        &change,
        pb::ThreadAnchor {
            anchor: Some(pb::thread_anchor::Anchor::File(pb::thread_anchor::File {
                version: v3.clone(),
                path: "pub/m.rs".into(),
            })),
        },
        None,
    )
    .await;
    let mut renamed = set_edit("pub/n.rs", b"x\ny\nz\nw\n");
    if let Some(pb::edit::Op::SetContent(set)) = &mut renamed.op {
        set.copy_from = Some(pb::CopySource {
            path: "pub/m.rs".into(),
            version: v3.clone(),
        })
        .into();
    }
    let delete = pb::Edit {
        path: "pub/m.rs".into(),
        op: Some(pb::edit::Op::Delete(true)),
    };
    amend(&mut bob_c, &change, vec![renamed, delete]).await;
    let state = review(&mut acl.owner, &change).await;
    let followed = state.threads.iter().find(|x| x.id == file.id).unwrap();
    let Some(pb::thread_anchor::Anchor::File(f)) = &followed.anchor.anchor else {
        panic!("{followed:?}")
    };
    assert_eq!(f.path, "pub/n.rs");
    assert!(!followed.outdated);

    bob_c
        .review
        .abandon_change(pb::AbandonChangeRequest {
            change: common::names::change("acl", &change),
            ..Default::default()
        })
        .await
        .unwrap();
    let reopened = bob_c
        .review
        .reopen_change(pb::ReopenChangeRequest {
            change: common::names::change("acl", &change),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .review
        .unwrap();
    assert_eq!(
        reopened.status,
        buffa::EnumValue::from(pb::ReviewStatus::Open)
    );

    // Trunk moves on, so the lander rebases: the review ends on the
    // version that landed.
    let other = super::writes::edit(&mut acl.owner, None, vec![set_edit("pub/other.rs", b"o\n")])
        .await
        .unwrap();
    let other_change = change_of(&mut acl.owner, &other).await;
    land_with(&mut acl.owner, "acl", other_change, &[("Owner", 1)]).await;
    for t in &review(&mut acl.owner, &change).await.threads {
        if !t.resolved {
            acl.owner
                .review
                .resolve_thread(pb::ResolveThreadRequest {
                    name: t.name.clone(),
                    resolved: true,
                    ..Default::default()
                })
                .await
                .unwrap();
        }
    }
    for (label, value) in [("Verified", 1), ("Review", 2), ("Owner", 1)] {
        vote(&mut acl.owner, &change, label, value).await.unwrap();
    }
    let seq = common::request_land(&mut acl.owner, "acl", change.clone())
        .await
        .unwrap();
    let landed = acl
        .owner
        .repo
        .get_trunk_position(pb::GetTrunkPositionRequest {
            name: common::names::position("acl", seq),
        })
        .await
        .unwrap()
        .into_owned()
        .landing
        .unwrap();
    let state = review(&mut acl.owner, &change).await;
    assert_eq!(state.versions.last(), Some(&landed.version_hash));
    acl.server.stop().await;
}
