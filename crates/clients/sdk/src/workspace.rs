//! Server-side workspaces: creation, snapshots, new changes, base updates,
//! handoff, undo and the operation log.

use rill_proto::convert::{hash_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{
    Operation, OperationId, Principal, RepoPathBuf, SparseProfile, TreeHash, Workspace, WorkspaceId,
};

use crate::error::Result;
use crate::names;
use crate::repo::RepoClient;
use crate::types::{
    DiffFormat, Edit, FileDiff, NewChange, NewChangeResult, Page, Root, UpdateBaseResult,
    WorkspaceFilter, WorkspaceOp,
};

/// Workspace operations on one repository.
#[derive(Clone, Debug)]
pub struct WorkspaceClient {
    owner: RepoClient,
    repo: String,
}

impl WorkspaceClient {
    pub(crate) fn new(owner: RepoClient) -> Self {
        let repo = owner.resource().to_string();
        WorkspaceClient { owner, repo }
    }

    fn svc(&self) -> crate::client::WorkspaceSvc {
        self.owner.client().workspace_svc()
    }

    fn name(&self, id: &WorkspaceId) -> String {
        format!("{}/workspaces/{id}", self.repo)
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
            request_id: names::request_id(&op),
            parent: self.repo.clone(),
            workspace: pb::Workspace {
                display_name: name.to_string(),
                sparse: pb::SparseProfile::from(sparse).into(),
                ..Default::default()
            }
            .into(),
            base: base.map(|b| b.to_string()).unwrap_or_default(),
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
                name: self.name(&workspace),
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
            parent: self.repo.clone(),
            page_size: filter.limit as i32,
            page_token: filter.page_token.clone().unwrap_or_default(),
            filter: crate::filter::FilterBuilder::new()
                .eq_opt("owner", filter.owner.as_deref())
                .build(),
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
        Ok(Page::new(
            items,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
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
        let request = pb::UpdateWorkspaceRequest {
            request_id: names::request_id(&op),
            workspace: pb::Workspace {
                name: self.name(&workspace),
                sparse: pb::SparseProfile::from(sparse).into(),
                ..Default::default()
            }
            .into(),
            etag: String::new(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_workspace(request).await }
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
        let request = pb::SnapshotWorkspaceRequest {
            request_id: names::request_id(&op),
            name: self.name(&workspace),
            source: Some(pb::snapshot_workspace_request::Source::TreeHash(
                hash_to_proto(&tree),
            )),
            etag: String::new(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.snapshot_workspace(request).await }
        })
        .await?
        .into_owned();
        Ok(required("snapshot_workspace.operation", resp.operation)?.try_into()?)
    }

    /// Record edits as the workspace's snapshot; the server builds the
    /// tree. `from_base` applies them to the base, replacing the snapshot,
    /// rather than to the current snapshot. `etag` makes it conditional on
    /// the workspace not having changed. Patch edits are refused.
    pub async fn snapshot_edits(
        &self,
        workspace: WorkspaceId,
        edits: Vec<Edit>,
        from_base: bool,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let mut wire = Vec::with_capacity(edits.len());
        for edit in edits {
            wire.push(self.owner.edit_to_proto(edit).await?);
        }
        let request = pb::SnapshotWorkspaceRequest {
            request_id: names::request_id(&OperationId::new()),
            name: self.name(&workspace),
            source: Some(pb::snapshot_workspace_request::Source::Edits(
                pb::WorkspaceEdits {
                    from_base,
                    edits: wire,
                },
            )),
            etag: etag.unwrap_or_default().to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.snapshot_workspace(request).await }
        })
        .await?
        .into_owned();
        Ok(required("snapshot_workspace.operation", resp.operation)?.try_into()?)
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
        let request = pb::StartChangeRequest {
            request_id: names::request_id(&op),
            name: self.name(&workspace),
            message: change.message,
            intent: change
                .intent
                .map(|i| names::intent(&self.repo, &i))
                .unwrap_or_default(),
            provenance: pb::Provenance::from(&change.provenance).into(),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.start_change(request).await }
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
        let request = pb::SyncWorkspaceRequest {
            request_id: names::request_id(&op),
            name: self.name(&workspace),
            base: new_base.map(|b| b.to_string()).unwrap_or_default(),
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.sync_workspace(request).await }
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
        let request = pb::HandoffWorkspaceRequest {
            request_id: names::request_id(&op),
            name: self.name(&workspace),
            to: pb::Principal::from(to).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.handoff_workspace(request).await }
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
        let target = target.map_or_else(|| "head".to_string(), |t| t.to_string());
        let request = pb::UndoOperationRequest {
            request_id: names::request_id(&op),
            name: format!("{}/operations/{target}", self.name(&workspace)),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.undo_operation(request).await }
        })
        .await?
        .into_owned();
        WorkspaceOp::from_parts(resp.workspace.into_option(), resp.operation.into_option())
    }

    /// One page of the operation log, newest first; pass the returned token
    /// to continue.
    pub async fn operations(
        &self,
        workspace: WorkspaceId,
        limit: u32,
        page_token: Option<&str>,
    ) -> Result<Page<Operation>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListWorkspaceOperationsRequest {
                parent: self.name(&workspace),
                page_size: limit as i32,
                page_token: page_token.unwrap_or_default().to_string(),
            };
            async move { svc.list_workspace_operations(request).await }
        })
        .await?
        .into_owned();
        let items = resp
            .operations
            .into_iter()
            .map(|o| Operation::try_from(o).map_err(Into::into))
            .collect::<Result<_>>()?;
        Ok(Page::new(
            items,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
    }

    /// Delete a workspace.
    pub async fn delete(&self, workspace: WorkspaceId) -> Result<()> {
        self.delete_with_op(OperationId::new(), workspace).await
    }

    /// [`WorkspaceClient::delete`] with a caller-chosen idempotency key.
    pub async fn delete_with_op(&self, op: OperationId, workspace: WorkspaceId) -> Result<()> {
        let request = pb::DeleteWorkspaceRequest {
            request_id: names::request_id(&op),
            name: self.name(&workspace),
            etag: String::new(),
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
            let request = pb::DiffWorkspaceRequest {
                name: self.name(&workspace),
                paths: paths.iter().map(|p| p.to_string()).collect(),
                format: format.to_proto().into(),
                context_lines,
            };
            async move { svc.diff_workspace(request).await }
        })
        .await?
        .into_owned();
        resp.files.into_iter().map(FileDiff::try_from).collect()
    }
}
