//! Writes: one check in the commit, again at landing, and owners'
//! approvals.

use rill_types::{ChangeVersion, FileChange, ObjectHash, RepoPathBuf, Timestamp};

use super::super::*;
use super::fixture::*;

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

async fn edit(c: &mut Clients, base: Option<String>, edits: Vec<pb::Edit>) -> Result<Bytes, Code> {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("acl"),
            base: (base.map(|revision| pb::EditBase {
                base: Some(pb::edit_base::Base::Revision(revision)),
            }))
            .into(),
            edits,
            message: "edit".into(),
            ..Default::default()
        })
        .await
        .map(|r| r.into_owned().info.unwrap().hash)
        .map_err(|s| s.code)
}

#[tokio::test]
async fn repository_configuration_takes_owner() {
    let mut acl = Acl::start().await;
    acl.grant("", "writer", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;
    let config = || {
        vec![set_edit(
            ".rill/owners.toml",
            b"[[rule]]\npath = \"\"\nowners = [\"user:bob\"]\n",
        )]
    };
    assert_eq!(
        edit(&mut c, Some(acl.at_version()), config()).await,
        Err(Code::PermissionDenied),
        "a writer cannot make itself owner of everything"
    );
    edit(
        &mut c,
        Some(acl.at_version()),
        vec![set_edit("pub/b.txt", b"b")],
    )
    .await
    .unwrap();
    let at = acl.at_version();
    edit(&mut acl.owner, Some(at), config()).await.unwrap();
    acl.server.stop().await;
}

#[tokio::test]
async fn a_merge_is_charged_only_for_what_it_writes() {
    let mut acl = Acl::start().await;
    acl.grant("pub", "writer", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;
    // Alice changes the secret on top of the seed; bob changes pub/.
    let at = acl.at_version();
    let theirs = edit(
        &mut acl.owner,
        Some(at),
        vec![set_edit("secret/s2.txt", b"more secret")],
    )
    .await
    .unwrap();
    let mine = edit(
        &mut c,
        Some(acl.at_version()),
        vec![set_edit("pub/b.txt", b"b")],
    )
    .await
    .unwrap();
    // Merging brings alice's change in without bob writing it.
    c.changes
        .merge_versions(pb::MergeVersionsRequest {
            parent: common::names::repo("acl"),
            left: ObjectHash::from_slice(&mine).unwrap().to_hex(),
            right: ObjectHash::from_slice(&theirs).unwrap().to_hex(),
            message: "merge".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    // A two-parent version that sets the secret to something neither
    // parent has is a write, and refused.
    let mut forged = ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: vec![
            ObjectHash::from_slice(&mine).unwrap(),
            ObjectHash::from_slice(&theirs).unwrap(),
        ],
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: bob(),
        provenance: Default::default(),
        intent: None,
        message: "forged merge".into(),
        created_at: Timestamp::now(),
        extra: Default::default(),
    };
    forged.file_changes.insert(
        path("secret/s2.txt"),
        FileChange::set(
            rill_types::ContentRef::inline(b"bob was here".to_vec()),
            rill_types::FileKind::Regular,
        ),
    );
    let err = c
        .changes
        .create_version(pb::CreateVersionRequest {
            parent: common::names::repo("acl"),
            version: pb::ChangeVersion::from(forged).into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
    acl.server.stop().await;
}

async fn change_of(c: &mut Clients, version: &Bytes) -> Bytes {
    c.changes
        .get_version(pb::GetVersionRequest {
            name: common::names::version("acl", version),
            view: pb::VersionView::Basic.into(),
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap()
        .info
        .unwrap()
        .change_id
}

async fn vote(c: &mut Clients, change: &Bytes, label: &str, value: i32) -> Result<(), Code> {
    c.review
        .vote(pb::VoteRequest {
            review: common::names::review("acl", change),
            label: label.into(),
            value,
            ..Default::default()
        })
        .await
        .map(|_| ())
        .map_err(|s| s.code)
}

#[tokio::test]
async fn a_revoke_between_commit_and_land_keeps_the_change_off_trunk() {
    let mut acl = Acl::start().await;
    acl.grant("pub", "writer", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;
    let version = edit(
        &mut c,
        Some(acl.at_version()),
        vec![set_edit("pub/b.txt", b"b")],
    )
    .await
    .unwrap();
    let change = change_of(&mut c, &version).await;
    acl.owner
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    vote(&mut acl.owner, &change, "Verified", 1).await.unwrap();
    vote(&mut acl.owner, &change, "Review", 2).await.unwrap();

    // Bob loses write on pub/ after committing.
    acl.owner
        .access
        .delete_relation(pb::DeleteRelationRequest {
            parent: common::names::repo("acl"),
            relation: (pb::Relation {
                path: "pub".into(),
                relation: "writer".into(),
                subject: format!("principal:{}", bob().id),
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let ticket = acl
        .owner
        .landing
        .request_land(pb::RequestLandRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
            onto: String::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .land_request
        .unwrap();
    let mut error = None;
    for _ in 0..100 {
        let status = acl
            .owner
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
            pb::LandStatus::Landed => panic!("a revoked author's change landed"),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                error = status.error;
                break;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    let error = error.expect("the landing was refused");
    assert!(error.contains("may no longer write"), "{error}");
    acl.server.stop().await;
}

#[tokio::test]
async fn owner_approvals_come_from_the_owners_file_or_the_owner_relation() {
    let mut acl = Acl::start().await;
    add_user(&acl.server, "ann", vec![], &[]).await;
    add_user(&acl.server, "carol", vec![], &[]).await;
    for who in ["ann", "carol"] {
        acl.grant("", "reviewer", &rill_types::Principal::human(who))
            .await;
    }
    acl.grant("pub", "writer", &bob()).await;
    // Ann owns pub/ by the file, landed on trunk by the repository owner.
    let owners = edit(
        &mut acl.owner,
        None,
        vec![set_edit(
            ".rill/owners.toml",
            b"[[rule]]\npath = \"pub\"\nowners = [\"user:ann\"]\n",
        )],
    )
    .await
    .unwrap();
    let owners_change = change_of(&mut acl.owner, &owners).await;
    land_with(&mut acl.owner, "acl", owners_change, &[("Owner", 1)]).await;

    let mut c = acl.as_principal(&bob()).await;
    let version = edit(&mut c, None, vec![set_edit("pub/b.txt", b"b")])
        .await
        .unwrap();
    let change = change_of(&mut c, &version).await;
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    let mut carol = acl
        .as_principal(&rill_types::Principal::human("carol"))
        .await;
    assert_eq!(
        vote(&mut carol, &change, "Owner", 1).await,
        Err(Code::PermissionDenied)
    );
    assert_eq!(
        vote(&mut carol, &change, "Owner", -1).await,
        Ok(()),
        "anyone may object"
    );
    let mut ann = acl.as_principal(&rill_types::Principal::human("ann")).await;
    vote(&mut ann, &change, "Owner", 1).await.unwrap();
    // The repository owner is overruled by the file for pub/.
    assert_eq!(
        vote(&mut acl.owner, &change, "Owner", 1).await,
        Err(Code::PermissionDenied)
    );
    acl.server.stop().await;
}
