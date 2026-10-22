//! The `ChangeService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ChangeService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ChangeService`.
#[derive(Clone)]
pub struct ChangeSvc {
    pub(super) state: Arc<AppState>,
}

impl ChangeSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ChangeSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ChangeService for ChangeSvc {
    async fn get_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetVersionRequest>,
    ) -> GrpcResult<pb::GetVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_version_impl(request).await?))
    }

    async fn batch_get_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BatchGetVersionsRequest>,
    ) -> GrpcResult<pb::BatchGetVersionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.batch_get_versions_impl(request).await?))
    }

    async fn list_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListVersionsRequest>,
    ) -> GrpcResult<pb::ListVersionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_versions_impl(request).await?))
    }

    async fn create_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateVersionRequest>,
    ) -> GrpcResult<pb::CreateVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_version_impl(r).await?))
            })
            .await?,
        )
    }

    async fn apply_edits(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ApplyEditsRequest>,
    ) -> GrpcResult<pb::ApplyEditsResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.apply_edits_impl(r).await?))
            })
            .await?,
        )
    }

    async fn rebase_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RebaseVersionRequest>,
    ) -> GrpcResult<pb::RebaseVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.rebase_version_impl(r).await?))
            })
            .await?,
        )
    }

    async fn merge_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::MergeVersionsRequest>,
    ) -> GrpcResult<pb::MergeVersionsResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.merge_versions_impl(r).await?))
            })
            .await?,
        )
    }

    async fn resolve_conflicts(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveConflictsRequest>,
    ) -> GrpcResult<pb::ResolveConflictsResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.resolve_conflicts_impl(r).await?))
            })
            .await?,
        )
    }
}
