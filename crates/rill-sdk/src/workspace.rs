//! Server-side workspaces: creation, snapshots, new changes, base updates,
//! handoff, undo and the operation log.

use rill_proto::convert::{hash_to_proto, id_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{
    Operation, OperationId, Principal, RepoPathBuf, SparseProfile, TreeHash, Workspace, WorkspaceId,
};

use crate::client::Client;
use crate::error::Result;
use crate::types::{
    DiffFormat, FileDiff, NewChange, NewChangeResult, Page, Root, UpdateBaseResult,
    WorkspaceFilter, WorkspaceOp,
};

/// Workspace operations on one repository.
#[derive(Clone, Debug)]
pub struct WorkspaceClient {
    client: Client,
    repo: String,
}

impl WorkspaceClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        WorkspaceClient { client, repo }
    }

    fn svc(&self) -> crate::client::WorkspaceSvc {
        self.client.workspace_svc()
    }

    /// Create a workspace based on `base` (trunk head when `None`).
    pub async fn create(
        &self,
        name: &str,
        base: Option<Root>,
        sparse: SparseProfile,
    ) -> Result<WorkspaceOp> {
        self.create_with_op(OperationId::new(), name, base, sparse)
            .await
    }

    /// [`WorkspaceClient::create`] with a caller-chosen idempotency key.
    pub async fn create_with_op(
        &self,
        op: OperationId,
        name: &str,
        base: Option<Root>,
        sparse: SparseProfile,
    ) -> Result<WorkspaceOp> {
        let resp = self
            .svc()
            .create_workspace(pb::CreateWorkspaceRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                name: name.to_string(),
                base: base.map(|b| b.to_proto()),
                sparse: Some(pb::SparseProfile::from(sparse)),
            })
            .await?
            .into_inner();
        WorkspaceOp::from_parts(resp.workspace, resp.operation)
    }

    /// A workspace by id.
    pub async fn get(&self, workspace: WorkspaceId) -> Result<Workspace> {
        let resp = self
            .svc()
            .get_workspace(pb::GetWorkspaceRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
            })
            .await?
            .into_inner();
        Ok(required("get_workspace.workspace", resp.workspace)?.try_into()?)
    }

    /// Workspaces matching a filter.
    pub async fn list(&self, filter: WorkspaceFilter) -> Result<Page<Workspace>> {
        let resp = self
            .svc()
            .list_workspaces(pb::ListWorkspacesRequest {
                repo: self.repo.clone(),
                owner: filter.owner.as_ref().map(id_to_proto),
                limit: filter.limit,
                page_token: filter.page_token,
            })
            .await?
            .into_inner();
        let items = resp
            .workspaces
            .into_iter()
            .map(|w| Workspace::try_from(w).map_err(Into::into))
            .collect::<Result<_>>()?;
        Ok(Page::new(items, resp.next_page_token))
    }

    /// Change which directories the workspace materializes.
    pub async fn set_sparse(
        &self,
        workspace: WorkspaceId,
        sparse: SparseProfile,
    ) -> Result<WorkspaceOp> {
        self.set_sparse_with_op(OperationId::new(), workspace, sparse)
            .await
    }

    /// [`WorkspaceClient::set_sparse`] with a caller-chosen idempotency key.
    pub async fn set_sparse_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        sparse: SparseProfile,
    ) -> Result<WorkspaceOp> {
        let resp = self
            .svc()
            .set_sparse_profile(pb::SetSparseProfileRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                sparse: Some(pb::SparseProfile::from(sparse)),
            })
            .await?
            .into_inner();
        WorkspaceOp::from_parts(resp.workspace, resp.operation)
    }

    /// Record an uploaded tree as the workspace's current snapshot.
    pub async fn snapshot(&self, workspace: WorkspaceId, tree: TreeHash) -> Result<Operation> {
        self.snapshot_with_op(OperationId::new(), workspace, tree)
            .await
    }

    /// [`WorkspaceClient::snapshot`] with a caller-chosen idempotency key.
    pub async fn snapshot_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        tree: TreeHash,
    ) -> Result<Operation> {
        let resp = self
            .svc()
            .snapshot(pb::SnapshotRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                tree_hash: hash_to_proto(&tree),
            })
            .await?
            .into_inner();
        Ok(required("snapshot.operation", resp.operation)?.try_into()?)
    }

    /// Turn the current snapshot into a new change.
    pub async fn new_change(
        &self,
        workspace: WorkspaceId,
        change: NewChange,
    ) -> Result<NewChangeResult> {
        self.new_change_with_op(OperationId::new(), workspace, change)
            .await
    }

    /// [`WorkspaceClient::new_change`] with a caller-chosen idempotency key.
    pub async fn new_change_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        change: NewChange,
    ) -> Result<NewChangeResult> {
        self.svc()
            .new_change(pb::NewChangeRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                message: change.message,
                intent: change.intent.as_ref().map(id_to_proto),
                provenance: Some(pb::Provenance::from(&change.provenance)),
            })
            .await?
            .into_inner()
            .try_into()
    }

    /// Move the workspace onto a new base (trunk head when `None`),
    /// rebasing the working change if there is one.
    pub async fn update_base(
        &self,
        workspace: WorkspaceId,
        new_base: Option<Root>,
    ) -> Result<UpdateBaseResult> {
        self.update_base_with_op(OperationId::new(), workspace, new_base)
            .await
    }

    /// [`WorkspaceClient::update_base`] with a caller-chosen idempotency key.
    pub async fn update_base_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        new_base: Option<Root>,
    ) -> Result<UpdateBaseResult> {
        self.svc()
            .update_base(pb::UpdateBaseRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                new_base: new_base.map(|b| b.to_proto()),
            })
            .await?
            .into_inner()
            .try_into()
    }

    /// Transfer the workspace to another principal.
    pub async fn handoff(&self, workspace: WorkspaceId, to: Principal) -> Result<WorkspaceOp> {
        self.handoff_with_op(OperationId::new(), workspace, to)
            .await
    }

    /// [`WorkspaceClient::handoff`] with a caller-chosen idempotency key.
    pub async fn handoff_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        to: Principal,
    ) -> Result<WorkspaceOp> {
        let resp = self
            .svc()
            .handoff(pb::HandoffRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                to: Some(pb::Principal::from(to)),
            })
            .await?
            .into_inner();
        WorkspaceOp::from_parts(resp.workspace, resp.operation)
    }

    /// Undo an operation (the head operation when `None`).
    pub async fn undo(
        &self,
        workspace: WorkspaceId,
        target: Option<OperationId>,
    ) -> Result<WorkspaceOp> {
        self.undo_with_op(OperationId::new(), workspace, target)
            .await
    }

    /// [`WorkspaceClient::undo`] with a caller-chosen idempotency key.
    pub async fn undo_with_op(
        &self,
        op: OperationId,
        workspace: WorkspaceId,
        target: Option<OperationId>,
    ) -> Result<WorkspaceOp> {
        let resp = self
            .svc()
            .undo(pb::UndoRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                target: target.as_ref().map(id_to_proto),
            })
            .await?
            .into_inner();
        WorkspaceOp::from_parts(resp.workspace, resp.operation)
    }

    /// The operation log, newest first, continuing from `before`.
    pub async fn operations(
        &self,
        workspace: WorkspaceId,
        limit: u32,
        before: Option<OperationId>,
    ) -> Result<Vec<Operation>> {
        let resp = self
            .svc()
            .list_operations(pb::ListOperationsRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                limit,
                before: before.as_ref().map(id_to_proto),
            })
            .await?
            .into_inner();
        resp.operations
            .into_iter()
            .map(|o| Operation::try_from(o).map_err(Into::into))
            .collect()
    }

    /// Delete a workspace.
    pub async fn delete(&self, workspace: WorkspaceId) -> Result<()> {
        self.delete_with_op(OperationId::new(), workspace).await
    }

    /// [`WorkspaceClient::delete`] with a caller-chosen idempotency key.
    pub async fn delete_with_op(&self, op: OperationId, workspace: WorkspaceId) -> Result<()> {
        self.svc()
            .delete_workspace(pb::DeleteWorkspaceRequest {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
            })
            .await?;
        Ok(())
    }

    /// Diff of the workspace's snapshot against its base.
    pub async fn diff(
        &self,
        workspace: WorkspaceId,
        paths: Vec<RepoPathBuf>,
        format: DiffFormat,
        context_lines: u32,
    ) -> Result<Vec<FileDiff>> {
        let resp = self
            .svc()
            .workspace_diff(pb::WorkspaceDiffRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                paths: paths.iter().map(|p| p.to_string()).collect(),
                format: format.to_proto(),
                context_lines,
            })
            .await?
            .into_inner();
        resp.files.into_iter().map(FileDiff::try_from).collect()
    }
}
