//! Authorization holes found in the phase 11 audit, each pinned by the
//! request that used to get through.

use rill_policy::{Capability, Verb};
use rill_types::{ChangeVersion, RepoPathBuf, SubtreeChange, Timestamp};

use super::super::*;
use super::fixture::*;

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test]
async fn grep_is_bounded_by_the_token_paths() {
    let acl = Acl::start().await;
    let narrowed = Capability::repo_wide(
        acl.repo_id,
        [Verb::Read],
        Timestamp::from_millis(Timestamp::now().millis() + 600_000),
    )
    .with_paths([path("pub")]);
    let token = acl.server.token_with(&alice(), &narrowed);
    let mut c = clients(&acl.server, Some(&token)).await;
    let hits = c
        .repo
        .grep(pb::GrepRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            pattern: CANARY.into(),
            case_insensitive: false,
            path_globs: vec![],
            max_results: 0,
            context_lines: 0,
        })
        .await
        .map(|r| r.into_inner().hits)
        .unwrap_or_default();
    assert!(hits.is_empty(), "a token for pub/ found {hits:?}");
    acl.server.stop().await;
}

fn subtree_version(acl: &Acl, at: &str, change: SubtreeChange) -> pb::ChangeVersion {
    let mut cv = ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: vec![rill_types::ObjectHash::from_slice(&acl.version).unwrap()],
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: bob(),
        provenance: Default::default(),
        intent: None,
        message: "subtree".into(),
        created_at: Timestamp::now(),
        extra: Default::default(),
    };
    cv.subtree_changes.insert(path(at), change);
    pb::ChangeVersion::from(cv)
}

#[tokio::test]
async fn subtree_changes_need_write_on_the_target_and_read_on_the_source() {
    let mut acl = Acl::start().await;
    acl.grant("dir:/pub", "writer", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;
    let attempts = [
        (
            "delete outside the writable directory",
            "secret",
            SubtreeChange::Delete,
        ),
        (
            "copy a directory the writer cannot read",
            "pub/stolen",
            SubtreeChange::Copy {
                from_path: path("secret"),
                from_version: rill_types::ObjectHash::from_slice(&acl.version).unwrap(),
            },
        ),
    ];
    for (what, at, change) in attempts {
        let result = c
            .repo
            .create_change_version(pb::CreateChangeVersionRequest {
                operation_id: Bytes::new(),
                repo: "acl".into(),
                version: Some(subtree_version(&acl, at, change)),
                signatures: Vec::new(),
            })
            .await;
        assert_eq!(
            result.map(|_| ()).unwrap_err().code(),
            Code::PermissionDenied,
            "{what}"
        );
    }
    // The same writer may delete inside its own directory.
    c.repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "acl".into(),
            version: Some(subtree_version(&acl, "pub", SubtreeChange::Delete)),
            signatures: Vec::new(),
        })
        .await
        .unwrap();
    acl.server.stop().await;
}
