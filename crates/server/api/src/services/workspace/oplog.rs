//! Loading workspaces and appending to their operation log.

use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeVersion, ChangeVersionHash, Operation, OperationId, OperationKind, RepoPathBuf,
    Timestamp, TreeHash, Workspace, WorkspaceId,
};

use super::WorkspaceSvc;
use crate::services::changes::{commit_version_with, CommitExtras};
use crate::services::common::{self, Opened};
use crate::services::names;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

impl WorkspaceSvc {
    /// Load a workspace the caller may modify.
    pub(super) async fn load_mut(&self, opened: &Opened, name: &str) -> Result<Workspace> {
        let ws = self.load(opened, name).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        opened.permit().owns_or_admin(&ws.owner).await?;
        Ok(ws)
    }

    pub(super) async fn load(&self, opened: &Opened, name: &str) -> Result<Workspace> {
        let id = workspace_id(name)?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        opened
            .repo
            .repo
            .get_workspace(&id)
            .await?
            .ok_or_else(|| Error::not_found(format!("workspace {id}")))
    }

    /// Append `kind` as the next operation and persist the workspace,
    /// checking it still equals `before` (absent for a new workspace).
    pub(super) async fn record(
        &self,
        opened: &Opened,
        before: Option<&Workspace>,
        ws: &mut Workspace,
        op_id: OperationId,
        kind: OperationKind,
    ) -> Result<Operation> {
        let op = operation(opened, before, ws, op_id, kind);
        opened
            .repo
            .repo
            .append_operation(before, &op)
            .await
            .map_err(|e| {
                if e.is_conflict() {
                    Error::AlreadyExists(format!(
                        "operation {} was already applied, or the workspace changed meanwhile",
                        op.id
                    ))
                } else {
                    e.into()
                }
            })?;
        Ok(op)
    }
}

/// The next operation on `ws`, which it updates to point at.
pub(super) fn operation(
    opened: &Opened,
    before: Option<&Workspace>,
    ws: &mut Workspace,
    id: OperationId,
    kind: OperationKind,
) -> Operation {
    let at = Timestamp::now();
    ws.head_op = Some(id);
    ws.updated_at = at;
    Operation {
        id,
        workspace: ws.id,
        parent: before.and_then(|b| b.head_op),
        actor: opened.principal().clone(),
        kind,
        resulting: ws.clone(),
        at,
    }
}

/// Store `cv` and the workspace operation together, so the version never
/// exists without the workspace pointing at it.
pub(super) async fn commit_with_operation(
    opened: &Opened,
    cv: ChangeVersion,
    before: &Workspace,
    op: &Operation,
) -> Result<(ChangeVersionHash, ChangeVersion)> {
    let tx = opened.repo.repo.operation_tx(Some(before), op)?;
    commit_version_with(
        opened,
        cv,
        CommitExtras {
            tx,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| match e {
        Error::Store(s) if s.is_conflict() => Error::AlreadyExists(format!(
            "operation {} was already applied, or the workspace changed meanwhile",
            op.id
        )),
        other => other,
    })
}

pub(super) fn base_of(ws: &Workspace) -> Option<ChangeVersionHash> {
    (!ws.base.is_zero()).then_some(ws.base)
}

pub(super) async fn base_root(opened: &Opened, ws: &Workspace) -> Result<Option<TreeHash>> {
    match base_of(ws) {
        Some(v) => Ok(Some(common::root_of(&opened.repo, &v).await?)),
        None => Ok(None),
    }
}

/// The workspace id a workspace (or workspace operation) name holds.
pub(super) fn workspace_id(name: &str) -> Result<WorkspaceId> {
    let ws = name
        .split('/')
        .nth(5)
        .filter(|_| name.split('/').nth(4) == Some("workspaces"))
        .ok_or_else(|| Error::InvalidName(format!("{name:?} is not a workspace name")))?;
    ws.parse()
        .map_err(|_| Error::InvalidName(format!("{ws:?} is not a workspace id")))
}

/// A workspace's etag: its head operation, which every change moves.
pub(super) fn etag(ws: &Workspace) -> String {
    ws.head_op.map(|op| op.to_string()).unwrap_or_default()
}

/// Refuse to change `ws` unless `etag` is its current one; an empty etag
/// is no condition.
pub(super) fn check_etag(ws: &Workspace, given: &str) -> Result<()> {
    if !given.is_empty() && given != etag(ws) {
        return Err(Error::EtagMismatch(format!(
            "workspace {} has changed; read it again",
            ws.id
        )));
    }
    Ok(())
}

/// The wire form of a workspace, with its name and etag.
pub(super) fn workspace_proto(ctx: &RepoContext, ws: &Workspace) -> pb::Workspace {
    pb::Workspace {
        name: names::workspace(ctx, &ws.id),
        etag: etag(ws),
        ..pb::Workspace::from(ws)
    }
}

/// The wire form of an operation, with its name.
pub(super) fn operation_proto(ctx: &RepoContext, op: &Operation) -> pb::WorkspaceOperation {
    pb::WorkspaceOperation {
        name: format!(
            "{}/operations/{}",
            names::workspace(ctx, &op.workspace),
            op.id
        ),
        resulting: workspace_proto(ctx, &op.resulting).into(),
        ..pb::WorkspaceOperation::from(op)
    }
}

pub(super) fn response_pair(
    ctx: &RepoContext,
    ws: &Workspace,
    op: &Operation,
) -> (Option<pb::Workspace>, Option<pb::WorkspaceOperation>) {
    (
        Some(workspace_proto(ctx, ws)),
        Some(operation_proto(ctx, op)),
    )
}
