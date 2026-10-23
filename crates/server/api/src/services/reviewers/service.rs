//! The `ReviewerService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ReviewerService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ReviewerService`.
#[derive(Clone)]
pub struct ReviewerSvc {
    pub(super) state: Arc<AppState>,
}

impl ReviewerSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ReviewerSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ReviewerService for ReviewerSvc {
    async fn lease_review_job(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LeaseReviewJobRequest>,
    ) -> GrpcResult<pb::LeaseReviewJobResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.lease_review_job_impl(request).await?))
    }

    async fn renew_review_lease(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RenewReviewLeaseRequest>,
    ) -> GrpcResult<pb::RenewReviewLeaseResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.renew_review_lease_impl(request).await?))
    }

    async fn complete_review_job(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CompleteReviewJobRequest>,
    ) -> GrpcResult<pb::CompleteReviewJobResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.complete_review_job_impl(r).await?))
            })
            .await?,
        )
    }

    async fn fail_review_job(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FailReviewJobRequest>,
    ) -> GrpcResult<pb::FailReviewJobResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.fail_review_job_impl(r).await?))
            })
            .await?,
        )
    }

    async fn request_review(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RequestReviewRequest>,
    ) -> GrpcResult<pb::RequestReviewResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.request_review_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_review_runs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReviewRunsRequest>,
    ) -> GrpcResult<pb::ListReviewRunsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_review_runs_impl(request).await?))
    }

    async fn lease_resolve_job(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LeaseResolveJobRequest>,
    ) -> GrpcResult<pb::LeaseResolveJobResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.lease_resolve_job_impl(request).await?))
    }

    async fn complete_resolve_job(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CompleteResolveJobRequest>,
    ) -> GrpcResult<pb::CompleteResolveJobResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.complete_resolve_job_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_reviewer_stats(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReviewerStatsRequest>,
    ) -> GrpcResult<pb::ListReviewerStatsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_reviewer_stats_impl(request).await?))
    }

    async fn reinstate_reviewer(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReinstateReviewerRequest>,
    ) -> GrpcResult<pb::ReinstateReviewerResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.reinstate_reviewer_impl(r).await?))
            })
            .await?,
        )
    }
}
