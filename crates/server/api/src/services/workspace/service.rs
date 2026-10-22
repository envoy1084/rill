//! The `WorkspaceService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::WorkspaceService;

use crate::services::common::GrpcResult;
use crate::services::idem;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
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

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl WorkspaceService for WorkspaceSvc {
    async fn create_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateWorkspaceRequest>,
    ) -> GrpcResult<pb::CreateWorkspaceResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.create_workspace_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn get_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetWorkspaceRequest>,
    ) -> GrpcResult<pb::GetWorkspaceResponse> {
        let request = Call::new(ctx, request);
        self.get_workspace_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn list_workspaces(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWorkspacesRequest>,
    ) -> GrpcResult<pb::ListWorkspacesResponse> {
        let request = Call::new(ctx, request);
        self.list_workspaces_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn set_sparse_profile(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SetSparseProfileRequest>,
    ) -> GrpcResult<pb::SetSparseProfileResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.set_sparse_profile_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn snapshot(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SnapshotRequest>,
    ) -> GrpcResult<pb::SnapshotResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.snapshot_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Turn the snapshot into a version of the working change. The
    /// workspace base then advances to that version and a fresh working
    /// change id is allocated, so the next `NewChange` stacks on top.
    async fn new_change(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::NewChangeRequest>,
    ) -> GrpcResult<pb::NewChangeResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.new_change_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Move the base; a working change with versions is rebased onto the
    /// new base and the snapshot follows the rebased tree.
    async fn update_base(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateBaseRequest>,
    ) -> GrpcResult<pb::UpdateBaseResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.update_base_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn handoff(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::HandoffRequest>,
    ) -> GrpcResult<pb::HandoffResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.handoff_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Restore the workspace to the state before `target` (default: the
    /// head operation), recorded as a new `Undo` operation.
    async fn undo(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UndoRequest>,
    ) -> GrpcResult<pb::UndoResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.undo_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_operations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListOperationsRequest>,
    ) -> GrpcResult<pb::ListOperationsResponse> {
        let request = Call::new(ctx, request);
        self.list_operations_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn delete_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteWorkspaceRequest>,
    ) -> GrpcResult<pb::DeleteWorkspaceResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.delete_workspace_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn workspace_diff(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WorkspaceDiffRequest>,
    ) -> GrpcResult<pb::WorkspaceDiffResponse> {
        let request = Call::new(ctx, request);
        self.workspace_diff_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }
}
