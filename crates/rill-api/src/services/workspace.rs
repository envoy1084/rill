//! `rill.v1.WorkspaceService`: server-side working copies with an
//! operation log.
//!
//! A workspace's `base` is `ObjectHash::ZERO` while the repository has no
//! history; every other base is a change version. Each mutating RPC
//! appends one [`Operation`] holding the whole workspace afterwards, which
//! is what `Undo` restores; undoing an `Undo` is a redo.

use std::sync::Arc;

use rill_core::{diff_trees, rebase};
use rill_proto::convert::{hash_to_proto, id_from_proto, id_to_proto, opt_id_from_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::workspace_service_server::WorkspaceService;
use rill_store::ObjectStoreExt;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, Encodable, ObjectHash, Operation, OperationId,
    OperationKind, Principal, RepoPathBuf, SparseProfile, Timestamp, TreeHash, Workspace,
    WorkspaceId,
};
use tonic::{Request, Response, Status};

use super::common::{self, open, operation_id, provenance, resolve_version, GrpcResult, Opened};
use super::diff::{file_diffs, DiffOptions};
use super::idem;
use super::repo::{commit_version_with, version_from_diff, CommitExtras};
use crate::error::{Error, Result};
use crate::state::AppState;

/// Default page size for operation listings.
const DEFAULT_OPS_LIMIT: usize = 50;
const MAX_OPS_LIMIT: usize = 1000;

/// The workspace service.
#[derive(Clone)]
pub struct WorkspaceSvc {
    state: Arc<AppState>,
}

impl WorkspaceSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        WorkspaceSvc { state }
    }

    /// Load a workspace the caller may modify.
    async fn load_mut(&self, opened: &Opened, id: &[u8]) -> Result<Workspace> {
        let ws = self.load(opened, id).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        opened.permit().owns_or_admin(&ws.owner).await?;
        Ok(ws)
    }

    async fn load(&self, opened: &Opened, id: &[u8]) -> Result<Workspace> {
        let id: WorkspaceId = id_from_proto("workspace", id)?;
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
    async fn record(
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
fn operation(
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
async fn commit_with_operation(
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

fn base_of(ws: &Workspace) -> Option<ChangeVersionHash> {
    (!ws.base.is_zero()).then_some(ws.base)
}

async fn base_root(opened: &Opened, ws: &Workspace) -> Result<Option<TreeHash>> {
    match base_of(ws) {
        Some(v) => Ok(Some(common::root_of(&opened.repo, &v).await?)),
        None => Ok(None),
    }
}

fn response_pair(ws: &Workspace, op: &Operation) -> (Option<pb::Workspace>, Option<pb::Operation>) {
    (Some(pb::Workspace::from(ws)), Some(pb::Operation::from(op)))
}

#[tonic::async_trait]
impl WorkspaceService for WorkspaceSvc {
    async fn create_workspace(
        &self,
        request: Request<pb::CreateWorkspaceRequest>,
    ) -> GrpcResult<pb::CreateWorkspaceResponse> {
        let result: crate::error::Result<Response<pb::CreateWorkspaceResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                opened.permit().write(&RepoPathBuf::root()).await?;
                let base = resolve_version(&opened.repo, req.base.as_ref())
                    .await?
                    .unwrap_or(ObjectHash::ZERO);
                let sparse = match req.sparse {
                    Some(s) => SparseProfile::try_from(s)?,
                    None => SparseProfile::full(),
                };
                let now = Timestamp::now();
                let mut ws = Workspace {
                    id: WorkspaceId::new(),
                    repo: opened.repo.id(),
                    owner: opened.principal().clone(),
                    name: req.name,
                    base,
                    working_change: ChangeId::new(),
                    snapshot: None,
                    sparse,
                    head_op: None,
                    created_at: now,
                    updated_at: now,
                };
                let op = self
                    .record(
                        &opened,
                        None,
                        &mut ws,
                        operation_id(&req.operation_id)?,
                        OperationKind::Create,
                    )
                    .await?;
                let (workspace, operation) = response_pair(&ws, &op);
                Ok(Response::new(pb::CreateWorkspaceResponse {
                    workspace,
                    operation,
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn get_workspace(
        &self,
        request: Request<pb::GetWorkspaceRequest>,
    ) -> GrpcResult<pb::GetWorkspaceResponse> {
        let result: crate::error::Result<Response<pb::GetWorkspaceResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            let ws = self.load(&opened, &request.get_ref().workspace).await?;
            Ok(Response::new(pb::GetWorkspaceResponse {
                workspace: Some(pb::Workspace::from(ws)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn list_workspaces(
        &self,
        request: Request<pb::ListWorkspacesRequest>,
    ) -> GrpcResult<pb::ListWorkspacesResponse> {
        let result: crate::error::Result<Response<pb::ListWorkspacesResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let owner: Option<rill_types::PrincipalId> =
                opt_id_from_proto("owner", req.owner.as_ref())?;
            let limit = common::limit(req.limit, 100, 10_000);
            let workspaces = opened
                .repo
                .repo
                .list_workspaces()
                .await?
                .into_iter()
                .filter(|w| owner.is_none_or(|o| w.owner.id == o))
                .take(limit)
                .map(pb::Workspace::from)
                .collect();
            Ok(Response::new(pb::ListWorkspacesResponse {
                workspaces,
                next_page_token: None,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn set_sparse_profile(
        &self,
        request: Request<pb::SetSparseProfileRequest>,
    ) -> GrpcResult<pb::SetSparseProfileResponse> {
        let result: crate::error::Result<Response<pb::SetSparseProfileResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let profile = match req.sparse {
                    Some(s) => SparseProfile::try_from(s)?,
                    None => SparseProfile::full(),
                };
                ws.sparse = profile.clone();
                let op = self
                    .record(
                        &opened,
                        Some(&before),
                        &mut ws,
                        operation_id(&req.operation_id)?,
                        OperationKind::SetSparse { profile },
                    )
                    .await?;
                let (workspace, operation) = response_pair(&ws, &op);
                Ok(Response::new(pb::SetSparseProfileResponse {
                    workspace,
                    operation,
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn snapshot(
        &self,
        request: Request<pb::SnapshotRequest>,
    ) -> GrpcResult<pb::SnapshotResponse> {
        let result: crate::error::Result<Response<pb::SnapshotResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let tree = common::hash("tree_hash", &req.tree_hash)?;
                opened.repo.trees.get_tree(&tree).await.map_err(|_| {
                    Error::precondition(format!("snapshot tree {tree} has not been uploaded"))
                })?;
                ws.snapshot = Some(tree);
                let op = self
                    .record(
                        &opened,
                        Some(&before),
                        &mut ws,
                        operation_id(&req.operation_id)?,
                        OperationKind::Snapshot { tree },
                    )
                    .await?;
                Ok(Response::new(pb::SnapshotResponse {
                    operation: Some(pb::Operation::from(op)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    /// Turn the snapshot into a version of the working change. The
    /// workspace base then advances to that version and a fresh working
    /// change id is allocated, so the next `NewChange` stacks on top.
    async fn new_change(
        &self,
        request: Request<pb::NewChangeRequest>,
    ) -> GrpcResult<pb::NewChangeResponse> {
        let result: crate::error::Result<Response<pb::NewChangeResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let snapshot = ws.snapshot.ok_or_else(|| {
                    Error::precondition("workspace has no snapshot to turn into a change")
                })?;
                let base_root = base_root(&opened, &ws).await?;
                let entries = diff_trees(&opened.repo.trees, base_root, Some(snapshot)).await?;
                if entries.is_empty() {
                    return Err(Error::precondition(
                        "snapshot is identical to the base; nothing to change",
                    ));
                }
                for entry in &entries {
                    opened.permit().write(&entry.path).await?;
                }
                let predecessor = opened
                    .repo
                    .repo
                    .change_versions(&ws.working_change)
                    .await?
                    .last()
                    .copied();
                let cv = version_from_diff(
                    entries,
                    ws.working_change,
                    base_of(&ws).into_iter().collect(),
                    predecessor,
                    opened.principal().clone(),
                    req.message,
                    provenance(req.provenance)?,
                    opt_id_from_proto("intent", req.intent.as_ref())?,
                );
                let change = cv.change_id;
                let base = ws.base;
                ws.base = cv.hash();
                ws.working_change = ChangeId::new();
                let op = operation(
                    &opened,
                    Some(&before),
                    &mut ws,
                    operation_id(&req.operation_id)?,
                    OperationKind::NewChange { change, base },
                );
                let (hash, cv) = commit_with_operation(&opened, cv, &before, &op).await?;
                Ok(Response::new(pb::NewChangeResponse {
                    change_id: id_to_proto(&change),
                    version_hash: hash_to_proto(&hash),
                    version: Some(pb::ChangeVersion::from(cv)),
                    operation: Some(pb::Operation::from(op)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    /// Move the base; a working change with versions is rebased onto the
    /// new base and the snapshot follows the rebased tree.
    async fn update_base(
        &self,
        request: Request<pb::UpdateBaseRequest>,
    ) -> GrpcResult<pb::UpdateBaseResponse> {
        let result: crate::error::Result<Response<pb::UpdateBaseResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let new_base = resolve_version(&opened.repo, req.new_base.as_ref())
                    .await?
                    .ok_or_else(|| Error::precondition("the repository has no trunk head yet"))?;
                let from = ws.base;
                let current = opened
                    .repo
                    .repo
                    .change_versions(&ws.working_change)
                    .await?
                    .last()
                    .copied();
                ws.base = new_base;
                let op_id = operation_id(&req.operation_id)?;
                let kind = OperationKind::Rebase { from, to: new_base };
                let (op, version, version_hash) = match current {
                    Some(current) if new_base != from => {
                        let rebased =
                            rebase(&opened.repo.derive, &current, &new_base, Timestamp::now())
                                .await?;
                        // The snapshot is the rebased version's tree, which
                        // needs the version object stored first.
                        let hash = opened.repo.repo.objects().put_object(&rebased).await?;
                        ws.snapshot = Some(common::root_of(&opened.repo, &hash).await?);
                        let op = operation(&opened, Some(&before), &mut ws, op_id, kind);
                        let (hash, cv) =
                            commit_with_operation(&opened, rebased, &before, &op).await?;
                        (
                            op,
                            Some(pb::ChangeVersion::from(cv)),
                            Some(hash_to_proto(&hash)),
                        )
                    }
                    _ => (
                        self.record(&opened, Some(&before), &mut ws, op_id, kind)
                            .await?,
                        None,
                        None,
                    ),
                };
                let (workspace, operation) = response_pair(&ws, &op);
                Ok(Response::new(pb::UpdateBaseResponse {
                    workspace,
                    operation,
                    version,
                    version_hash,
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn handoff(
        &self,
        request: Request<pb::HandoffRequest>,
    ) -> GrpcResult<pb::HandoffResponse> {
        let result: crate::error::Result<Response<pb::HandoffResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let to =
                    Principal::try_from(req.to.ok_or_else(|| Error::invalid("to is required"))?)?;
                ws.owner = to.clone();
                let op = self
                    .record(
                        &opened,
                        Some(&before),
                        &mut ws,
                        operation_id(&req.operation_id)?,
                        OperationKind::Handoff { to },
                    )
                    .await?;
                let (workspace, operation) = response_pair(&ws, &op);
                Ok(Response::new(pb::HandoffResponse {
                    workspace,
                    operation,
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    /// Restore the workspace to the state before `target` (default: the
    /// head operation), recorded as a new `Undo` operation.
    async fn undo(&self, request: Request<pb::UndoRequest>) -> GrpcResult<pb::UndoResponse> {
        let result: crate::error::Result<Response<pb::UndoResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let req = request.into_inner();
                let before = self.load_mut(&opened, &req.workspace).await?;
                let mut ws = before.clone();
                let ops = opened
                    .repo
                    .repo
                    .list_operations(&ws.id, MAX_OPS_LIMIT)
                    .await?;
                let target: OperationId = match opt_id_from_proto("target", req.target.as_ref())? {
                    Some(id) => id,
                    None => ws
                        .head_op
                        .ok_or_else(|| Error::precondition("workspace has no operations"))?,
                };
                let undone = ops
                    .iter()
                    .find(|o| o.id == target)
                    .ok_or_else(|| Error::not_found(format!("operation {target}")))?;
                let parent_id = undone.parent.ok_or_else(|| {
                    Error::precondition("the workspace's first operation cannot be undone")
                })?;
                let previous = ops.iter().find(|o| o.id == parent_id).ok_or_else(|| {
                    Error::not_found(format!("operation {parent_id} is beyond the retained log"))
                })?;
                // Everything but identity comes back: base, snapshot, working
                // change, sparse profile and owner.
                ws = Workspace {
                    id: ws.id,
                    repo: ws.repo,
                    created_at: ws.created_at,
                    head_op: ws.head_op,
                    updated_at: ws.updated_at,
                    ..previous.resulting.clone()
                };
                let op = self
                    .record(
                        &opened,
                        Some(&before),
                        &mut ws,
                        operation_id(&req.operation_id)?,
                        OperationKind::Undo { undone: target },
                    )
                    .await?;
                let (workspace, operation) = response_pair(&ws, &op);
                Ok(Response::new(pb::UndoResponse {
                    workspace,
                    operation,
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn list_operations(
        &self,
        request: Request<pb::ListOperationsRequest>,
    ) -> GrpcResult<pb::ListOperationsResponse> {
        let result: crate::error::Result<Response<pb::ListOperationsResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            let req = request.get_ref();
            let ws = self.load(&opened, &req.workspace).await?;
            let limit = common::limit(req.limit, DEFAULT_OPS_LIMIT, MAX_OPS_LIMIT);
            let before: Option<OperationId> = opt_id_from_proto("before", req.before.as_ref())?;
            let mut ops = opened
                .repo
                .repo
                .list_operations(&ws.id, MAX_OPS_LIMIT)
                .await?;
            if let Some(before) = before {
                if let Some(pos) = ops.iter().position(|o| o.id == before) {
                    ops.truncate(pos);
                }
            }
            let start = ops.len().saturating_sub(limit);
            let operations = ops.drain(start..).map(pb::Operation::from).collect();
            Ok(Response::new(pb::ListOperationsResponse { operations }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn delete_workspace(
        &self,
        request: Request<pb::DeleteWorkspaceRequest>,
    ) -> GrpcResult<pb::DeleteWorkspaceResponse> {
        let result: crate::error::Result<Response<pb::DeleteWorkspaceResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                let ws = self.load_mut(&opened, &request.get_ref().workspace).await?;
                opened
                    .repo
                    .repo
                    .delete_workspace(&ws.id, opened.principal())
                    .await?;
                Ok(Response::new(pb::DeleteWorkspaceResponse {}))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn workspace_diff(
        &self,
        request: Request<pb::WorkspaceDiffRequest>,
    ) -> GrpcResult<pb::WorkspaceDiffResponse> {
        let result: crate::error::Result<Response<pb::WorkspaceDiffResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            let req = request.get_ref();
            let ws = self.load(&opened, &req.workspace).await?;
            let filter = opened.permit().read_filter().await?;
            let options = DiffOptions::new(&req.paths, req.format, req.context_lines, 0, filter)?;
            let base_root = base_root(&opened, &ws).await?;
            let (files, _) =
                file_diffs(&opened.repo, base_root, ws.snapshot, None, &options).await?;
            Ok(Response::new(pb::WorkspaceDiffResponse { files }))
        }
        .await;
        result.map_err(Status::from)
    }
}
