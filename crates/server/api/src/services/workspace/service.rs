//! The `WorkspaceService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::workspace_service_server::WorkspaceService;
use tonic::{Request, Response, Status};

use crate::services::common::GrpcResult;
use crate::services::idem;
use rill_runtime::state::AppState;

/// The workspace service.
#[derive(Clone)]
pub struct WorkspaceSvc {
    pub(super) state: Arc<AppState>,
}

impl WorkspaceSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        WorkspaceSvc { state }
    }
}

#[tonic::async_trait]
impl WorkspaceService for WorkspaceSvc {
    async fn create_workspace(
        &self,
        request: Request<pb::CreateWorkspaceRequest>,
    ) -> GrpcResult<pb::CreateWorkspaceResponse> {
        idem::once(&self.state, request, |r| async move {
            self.create_workspace_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn get_workspace(
        &self,
        request: Request<pb::GetWorkspaceRequest>,
    ) -> GrpcResult<pb::GetWorkspaceResponse> {
        self.get_workspace_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn list_workspaces(
        &self,
        request: Request<pb::ListWorkspacesRequest>,
    ) -> GrpcResult<pb::ListWorkspacesResponse> {
        self.list_workspaces_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn set_sparse_profile(
        &self,
        request: Request<pb::SetSparseProfileRequest>,
    ) -> GrpcResult<pb::SetSparseProfileResponse> {
        idem::once(&self.state, request, |r| async move {
            self.set_sparse_profile_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn snapshot(
        &self,
        request: Request<pb::SnapshotRequest>,
    ) -> GrpcResult<pb::SnapshotResponse> {
        idem::once(&self.state, request, |r| async move {
            self.snapshot_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Turn the snapshot into a version of the working change. The
    /// workspace base then advances to that version and a fresh working
    /// change id is allocated, so the next `NewChange` stacks on top.
    async fn new_change(
        &self,
        request: Request<pb::NewChangeRequest>,
    ) -> GrpcResult<pb::NewChangeResponse> {
        idem::once(&self.state, request, |r| async move {
            self.new_change_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Move the base; a working change with versions is rebased onto the
    /// new base and the snapshot follows the rebased tree.
    async fn update_base(
        &self,
        request: Request<pb::UpdateBaseRequest>,
    ) -> GrpcResult<pb::UpdateBaseResponse> {
        idem::once(&self.state, request, |r| async move {
            self.update_base_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn handoff(
        &self,
        request: Request<pb::HandoffRequest>,
    ) -> GrpcResult<pb::HandoffResponse> {
        idem::once(&self.state, request, |r| async move {
            self.handoff_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Restore the workspace to the state before `target` (default: the
    /// head operation), recorded as a new `Undo` operation.
    async fn undo(&self, request: Request<pb::UndoRequest>) -> GrpcResult<pb::UndoResponse> {
        idem::once(&self.state, request, |r| async move {
            self.undo_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_operations(
        &self,
        request: Request<pb::ListOperationsRequest>,
    ) -> GrpcResult<pb::ListOperationsResponse> {
        self.list_operations_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn delete_workspace(
        &self,
        request: Request<pb::DeleteWorkspaceRequest>,
    ) -> GrpcResult<pb::DeleteWorkspaceResponse> {
        idem::once(&self.state, request, |r| async move {
            self.delete_workspace_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn workspace_diff(
        &self,
        request: Request<pb::WorkspaceDiffRequest>,
    ) -> GrpcResult<pb::WorkspaceDiffResponse> {
        self.workspace_diff_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }
}
