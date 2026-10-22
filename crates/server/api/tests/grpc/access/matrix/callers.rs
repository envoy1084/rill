//! The repository under test and a token for every role.

use rill_policy::{Capability, Verb};
use rill_types::{Principal, RepoPathBuf, Timestamp};

use super::super::fixture::*;
use super::roles::Role;
use crate::*;

pub(super) struct Callers {
    pub(super) acl: Acl,
    pub(super) tokens: Vec<(Role, Option<String>)>,
    pub(super) workspace: Bytes,
}

pub(super) async fn callers() -> Callers {
    let mut acl = Acl::start().await;
    // Trunk holds both directories, so requests that default to it read
    // real content.
    let change = acl
        .owner
        .repo
        .get_change_version(pb::GetChangeVersionRequest {
            repo: "acl".into(),
            hash: acl.version.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .version
        .unwrap()
        .change_id;
    land(&mut acl.owner, "acl", change).await;
    let carl = Principal::human("carl");
    acl.grant("pub", "reader", &bob()).await;
    acl.grant("pub", "writer", &carl).await;
    let narrowed = Capability::repo_wide(
        acl.repo_id,
        [Verb::Read],
        Timestamp::from_millis(Timestamp::now().millis() + 600_000),
    )
    .with_paths([RepoPathBuf::new("pub").unwrap()]);
    let s = &acl.server;
    let tokens = vec![
        (Role::Anonymous, None),
        (Role::Stranger, Some(s.token_for(&Principal::human("sam")))),
        (Role::PubReader, Some(s.token_for(&bob()))),
        (Role::PubWriter, Some(s.token_for(&carl))),
        (Role::Narrowed, Some(s.token_with(&alice(), &narrowed))),
        (Role::Owner, Some(s.token_for(&alice()))),
    ];
    let workspace = acl
        .owner
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: "acl".into(),
            name: "probe".into(),
            base: None,
            sparse: None,
        })
        .await
        .unwrap()
        .into_inner()
        .workspace
        .unwrap()
        .id;
    Callers {
        acl,
        tokens,
        workspace,
    }
}
