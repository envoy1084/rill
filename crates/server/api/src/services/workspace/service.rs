//! The `WorkspaceService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::WorkspaceService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `WorkspaceService`.
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
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_workspace_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetWorkspaceRequest>,
    ) -> GrpcResult<pb::GetWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_workspace_impl(request).await?))
    }

    async fn list_workspaces(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWorkspacesRequest>,
    ) -> GrpcResult<pb::ListWorkspacesResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_workspaces_impl(request).await?))
    }

    async fn update_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateWorkspaceRequest>,
    ) -> GrpcResult<pb::UpdateWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_workspace_impl(r).await?))
            })
            .await?,
        )
    }

    async fn delete_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteWorkspaceRequest>,
    ) -> GrpcResult<pb::DeleteWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.delete_workspace_impl(r).await?))
            })
            .await?,
        )
    }

    async fn snapshot_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SnapshotWorkspaceRequest>,
    ) -> GrpcResult<pb::SnapshotWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.snapshot_workspace_impl(request).await?))
    }

    async fn start_change(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::StartChangeRequest>,
    ) -> GrpcResult<pb::StartChangeResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.start_change_impl(r).await?))
            })
            .await?,
        )
    }

    async fn sync_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SyncWorkspaceRequest>,
    ) -> GrpcResult<pb::SyncWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.sync_workspace_impl(r).await?))
            })
            .await?,
        )
    }

    async fn handoff_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::HandoffWorkspaceRequest>,
    ) -> GrpcResult<pb::HandoffWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.handoff_workspace_impl(r).await?))
            })
            .await?,
        )
    }

    async fn undo_operation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UndoOperationRequest>,
    ) -> GrpcResult<pb::UndoOperationResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.undo_operation_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_workspace_operations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWorkspaceOperationsRequest>,
    ) -> GrpcResult<pb::ListWorkspaceOperationsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.list_workspace_operations_impl(request).await?,
        ))
    }

    async fn diff_workspace(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DiffWorkspaceRequest>,
    ) -> GrpcResult<pb::DiffWorkspaceResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.diff_workspace_impl(request).await?))
    }
}
