//! The `ReviewService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ReviewService;

use crate::services::common::GrpcResult;
use crate::services::repo::EventStream;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ReviewService`.
#[derive(Clone)]
pub struct ReviewSvc {
    pub(super) state: Arc<AppState>,
}

impl ReviewSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ReviewSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ReviewService for ReviewSvc {
    async fn get_review(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetReviewRequest>,
    ) -> GrpcResult<pb::GetReviewResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_review_impl(request).await?))
    }

    async fn batch_get_reviews(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BatchGetReviewsRequest>,
    ) -> GrpcResult<pb::BatchGetReviewsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.batch_get_reviews_impl(request).await?))
    }

    async fn list_reviews(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReviewsRequest>,
    ) -> GrpcResult<pb::ListReviewsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_reviews_impl(request).await?))
    }

    async fn submit_change(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SubmitChangeRequest>,
    ) -> GrpcResult<pb::SubmitChangeResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.submit_change_impl(r).await?))
            })
            .await?,
        )
    }

    async fn vote(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::VoteRequest>,
    ) -> GrpcResult<pb::VoteResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.vote_impl(r).await?))
            })
            .await?,
        )
    }

    async fn create_thread(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateThreadRequest>,
    ) -> GrpcResult<pb::CreateThreadResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_thread_impl(r).await?))
            })
            .await?,
        )
    }

    async fn reply_thread(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReplyThreadRequest>,
    ) -> GrpcResult<pb::ReplyThreadResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.reply_thread_impl(r).await?))
            })
            .await?,
        )
    }

    async fn resolve_thread(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveThreadRequest>,
    ) -> GrpcResult<pb::ResolveThreadResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.resolve_thread_impl(r).await?))
            })
            .await?,
        )
    }

    async fn update_stack(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateStackRequest>,
    ) -> GrpcResult<pb::UpdateStackResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_stack_impl(r).await?))
            })
            .await?,
        )
    }

    async fn evaluate_landing(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::EvaluateLandingRequest>,
    ) -> GrpcResult<pb::EvaluateLandingResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.evaluate_landing_impl(request).await?))
    }

    async fn abandon_change(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::AbandonChangeRequest>,
    ) -> GrpcResult<pb::AbandonChangeResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.abandon_change_impl(r).await?))
            })
            .await?,
        )
    }

    async fn watch_review(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchReviewRequest>,
    ) -> GrpcResult<EventStream<pb::WatchReviewResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_review_impl(request).await?))
    }
}
