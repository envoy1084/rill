//! Intents v2: an intent's scope is approved by the owners of the
//! directories it covers, and changes inside it inherit the approval.

use super::super::*;
use super::fixture::*;
use super::writes::change_of;
use crate::common::trunk_base;

pub(super) async fn create_intent(c: &mut Clients, scope: pb::IntentScope) -> pb::Intent {
    c.intents
        .create_intent(pb::CreateIntentRequest {
            parent: common::names::repo("acl"),
            title: "migrate the service".into(),
            kind: pb::IntentKind::Refactor.into(),
            scope: Some(scope).into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .intent
        .unwrap()
}

pub(super) async fn approve(c: &mut Clients, intent: &str, revision: u32) -> Result<(), Code> {
    c.intents
        .approve_intent(pb::ApproveIntentRequest {
            name: intent.into(),
            revision,
            ..Default::default()
        })
        .await
        .map(|_| ())
        .map_err(|s| s.code)
}

async fn explain(c: &mut Clients, change: &Bytes) -> pb::ExplainIntentScopeResponse {
    c.intents
        .explain_intent_scope(pb::ExplainIntentScopeRequest {
            change: common::names::change("acl", change),
        })
        .await
        .unwrap()
        .into_owned()
}

/// A change by `c` writing `paths`, linked to `intent`, submitted.
pub(super) async fn change_under(c: &mut Clients, intent: &str, paths: &[&str]) -> Bytes {
    let version = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("acl"),
            base: Some(trunk_base()).into(),
            edits: paths.iter().map(|p| set_edit(p, b"fn x() {}\n")).collect(),
            message: "under the intent".into(),
            intent: intent.into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash;
    let change = change_of(c, &version).await;
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    change
}

#[tokio::test]
async fn owners_approve_a_scope_and_changes_inside_it_inherit_the_approval() {
    let mut acl = Acl::start().await;
    for (who, teams) in [("ann", &["gw"][..]), ("carol", &[])] {
        add_user(&acl.server, who, vec![], teams).await;
        acl.grant("", "reviewer", &rill_types::Principal::human(who))
            .await;
    }
    acl.grant("svc", "writer", &bob()).await;
    let owners = edit_as(
        &mut acl.owner,
        vec![set_edit("svc/OWNERS.toml", b"owners = [\"team:gw\"]\n")],
    )
    .await;
    let owners_change = change_of(&mut acl.owner, &owners).await;
    land_with(&mut acl.owner, "acl", owners_change, &[("Owner", 1)]).await;

    let intent = create_intent(
        &mut acl.owner,
        pb::IntentScope {
            paths: vec!["svc/**".into()],
            forbid_paths: vec!["svc/secrets/**".into()],
            max_files_per_change: Some(5),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(intent.revision, 1);

    let mut bob_c = acl.as_principal(&bob()).await;
    let change = change_under(&mut bob_c, &intent.name, &["svc/a.rs"]).await;
    let before = explain(&mut acl.owner, &change).await;
    assert_eq!(before.intent, intent.name);
    assert!(!before.approved && !before.in_scope, "{before:?}");
    assert_eq!(before.missing_approvals, ["svc"]);

    let mut carol = acl
        .as_principal(&rill_types::Principal::human("carol"))
        .await;
    assert_eq!(
        approve(&mut carol, &intent.name, 1).await,
        Err(Code::PermissionDenied)
    );
    let mut ann = acl.as_principal(&rill_types::Principal::human("ann")).await;
    approve(&mut ann, &intent.name, 1).await.unwrap();
    let after = explain(&mut acl.owner, &change).await;
    assert!(after.approved && after.in_scope, "{after:?}");

    let forbidden = change_under(&mut bob_c, &intent.name, &["svc/secrets/k.rs"]).await;
    let out = explain(&mut acl.owner, &forbidden).await;
    assert!(out.approved && !out.in_scope, "{out:?}");
    assert!(
        out.reasons.iter().any(|r| r.contains("forbidden")),
        "{out:?}"
    );

    // Widening the scope is a new revision, which needs approving again.
    let mut widened = intent.clone();
    widened.scope = Some(pb::IntentScope {
        paths: vec!["svc/**".into(), "lib/**".into()],
        ..Default::default()
    })
    .into();
    let updated = acl
        .owner
        .intents
        .update_intent(pb::UpdateIntentRequest {
            intent: Some(widened).into(),
            update_mask: (rill_proto::FieldMask {
                paths: vec!["scope".into()],
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .intent
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert!(!explain(&mut acl.owner, &change).await.approved);
    assert_eq!(
        approve(&mut ann, &intent.name, 1).await,
        Err(Code::FailedPrecondition),
        "approving a stale revision"
    );

    // The budget consumer links changes named by their versions.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let got = acl
            .owner
            .intents
            .get_intent(pb::GetIntentRequest {
                name: intent.name.clone(),
            })
            .await
            .unwrap()
            .into_owned()
            .intent
            .unwrap();
        if got.changes.contains(&change) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "not linked: {:?}",
            got.changes
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    acl.server.stop().await;
}

async fn edit_as(c: &mut Clients, edits: Vec<pb::Edit>) -> Bytes {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("acl"),
            base: Some(trunk_base()).into(),
            edits,
            message: "edit".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash
}
