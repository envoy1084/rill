//! The `RepoService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;

use crate::services::common::GrpcResult;
use crate::services::repo::EventStream;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `RepoService`.
#[derive(Clone)]
pub struct RepoSvc {
    pub(super) state: Arc<AppState>,
}

impl RepoSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        RepoSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl RepoService for RepoSvc {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> GrpcResult<pb::GetRepoResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_repo_impl(request).await?))
    }

    async fn list_repos(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReposRequest>,
    ) -> GrpcResult<pb::ListReposResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_repos_impl(request).await?))
    }

    async fn create_repo(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateRepoRequest>,
    ) -> GrpcResult<pb::CreateRepoResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_repo_impl(r).await?))
            })
            .await?,
        )
    }

    async fn resolve_revision(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveRevisionRequest>,
    ) -> GrpcResult<pb::ResolveRevisionResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.resolve_revision_impl(request).await?))
    }

    async fn get_ref(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRefRequest>,
    ) -> GrpcResult<pb::GetRefResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_ref_impl(request).await?))
    }

    async fn list_refs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRefsRequest>,
    ) -> GrpcResult<pb::ListRefsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_refs_impl(request).await?))
    }

    async fn update_ref(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateRefRequest>,
    ) -> GrpcResult<pb::UpdateRefResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_ref_impl(r).await?))
            })
            .await?,
        )
    }

    async fn delete_ref(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteRefRequest>,
    ) -> GrpcResult<pb::DeleteRefResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.delete_ref_impl(r).await?))
            })
            .await?,
        )
    }

    async fn create_release(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateReleaseRequest>,
    ) -> GrpcResult<pb::CreateReleaseResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.create_release_impl(request).await?))
    }

    async fn get_trunk_position(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTrunkPositionRequest>,
    ) -> GrpcResult<pb::GetTrunkPositionResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_trunk_position_impl(request).await?))
    }

    async fn list_trunk_positions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListTrunkPositionsRequest>,
    ) -> GrpcResult<pb::ListTrunkPositionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.list_trunk_positions_impl(request).await?,
        ))
    }

    async fn watch_trunk(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchTrunkRequest>,
    ) -> GrpcResult<EventStream<pb::WatchTrunkResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_trunk_impl(request).await?))
    }

    async fn watch_refs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchRefsRequest>,
    ) -> GrpcResult<EventStream<pb::WatchRefsResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_refs_impl(request).await?))
    }

    async fn watch_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchVersionsRequest>,
    ) -> GrpcResult<EventStream<pb::WatchVersionsResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_versions_impl(request).await?))
    }

    async fn get_derived_value(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetDerivedValueRequest>,
    ) -> GrpcResult<pb::GetDerivedValueResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_derived_value_impl(request).await?))
    }

    async fn wait_derived_value(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WaitDerivedValueRequest>,
    ) -> GrpcResult<pb::WaitDerivedValueResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.wait_derived_value_impl(request).await?))
    }
}
