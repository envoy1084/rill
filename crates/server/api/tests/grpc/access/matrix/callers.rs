//! The repository under test and a token for every role.

use rill_policy::{Capability, Verb};
use rill_types::{Principal, RepoPathBuf, Timestamp};

use super::super::fixture::*;
use super::roles::Role;
use super::targets::Targets;
use crate::*;

pub(super) struct Callers {
    pub(super) acl: Acl,
    pub(super) tokens: Vec<(Role, Option<String>)>,
    pub(super) targets: Targets,
}

pub(super) async fn callers() -> Callers {
    let mut acl = Acl::start().await;
    // Trunk holds both directories, so requests that default to it read
    // real content.
    let change = acl
        .owner
        .changes
        .get_version(pb::GetVersionRequest {
            name: common::names::version("acl", &acl.version),
            view: pb::VersionView::Basic.into(),
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap()
        .info
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
            parent: common::names::repo("acl"),
            workspace: (pb::Workspace {
                display_name: "probe".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .workspace
        .unwrap()
        .name;
    let targets = Targets::new(&acl, workspace);
    Callers {
        acl,
        tokens,
        targets,
    }
}
