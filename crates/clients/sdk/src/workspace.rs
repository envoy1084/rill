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
        let request = pb::CreateWorkspaceRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            name: name.to_string(),
            base: (base.map(|b| b.to_proto())).into(),
            sparse: pb::SparseProfile::from(sparse).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_workspace(request).await }
        })
        .await?
        .into_owned();
        WorkspaceOp::from_parts(resp.workspace.into_option(), resp.operation.into_option())
    }

    /// A workspace by id.
    pub async fn get(&self, workspace: WorkspaceId) -> Result<Workspace> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetWorkspaceRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
            };
            async move { svc.get_workspace(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_workspace.workspace", resp.workspace)?.try_into()?)
    }

    /// Workspaces matching a filter.
    pub async fn list(&self, filter: WorkspaceFilter) -> Result<Page<Workspace>> {
        let request = pb::ListWorkspacesRequest {
            repo: self.repo.clone(),
            owner: filter.owner.as_ref().map(id_to_proto),
            limit: filter.limit,
            page_token: filter.page_token,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_workspaces(request).await }
        })
        .await?
        .into_owned();
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
        let request = pb::SetSparseProfileRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            sparse: pb::SparseProfile::from(sparse).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.set_sparse_profile(request).await }
        })
        .await?
        .into_owned();
        WorkspaceOp::from_parts(resp.workspace.into_option(), resp.operation.into_option())
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
        let request = pb::SnapshotRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            tree_hash: hash_to_proto(&tree),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.snapshot(request).await }
        })
        .await?
        .into_owned();
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
        let request = pb::NewChangeRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            message: change.message,
            intent: change.intent.as_ref().map(id_to_proto),
            provenance: pb::Provenance::from(&change.provenance).into(),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.new_change(request).await }
        })
        .await?
        .into_owned()
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
        let request = pb::UpdateBaseRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            new_base: (new_base.map(|b| b.to_proto())).into(),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_base(request).await }
        })
        .await?
        .into_owned()
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
        let request = pb::HandoffRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            to: pb::Principal::from(to).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.handoff(request).await }
        })
        .await?
        .into_owned();
        WorkspaceOp::from_parts(resp.workspace.into_option(), resp.operation.into_option())
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
        let request = pb::UndoRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
            target: target.as_ref().map(id_to_proto),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.undo(request).await }
        })
        .await?
        .into_owned();
        WorkspaceOp::from_parts(resp.workspace.into_option(), resp.operation.into_option())
    }

    /// The operation log, newest first, continuing from `before`.
    pub async fn operations(
        &self,
        workspace: WorkspaceId,
        limit: u32,
        before: Option<OperationId>,
    ) -> Result<Vec<Operation>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListOperationsRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                limit,
                before: before.as_ref().map(id_to_proto),
            };
            async move { svc.list_operations(request).await }
        })
        .await?
        .into_owned();
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
        let request = pb::DeleteWorkspaceRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            workspace: id_to_proto(&workspace),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_workspace(request).await }
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
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::WorkspaceDiffRequest {
                repo: self.repo.clone(),
                workspace: id_to_proto(&workspace),
                paths: paths.iter().map(|p| p.to_string()).collect(),
                format: format.to_proto().into(),
                context_lines,
            };
            async move { svc.workspace_diff(request).await }
        })
        .await?
        .into_owned();
        resp.files.into_iter().map(FileDiff::try_from).collect()
    }
}
