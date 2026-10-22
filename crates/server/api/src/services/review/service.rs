//! The `ReviewService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ReviewService;

use super::ReviewEventStream;
use crate::services::common::GrpcResult;
use crate::services::idem;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::state::AppState;

/// The review service.
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
        self.get_review_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn list_reviews(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReviewsRequest>,
    ) -> GrpcResult<pb::ListReviewsResponse> {
        let request = Call::new(ctx, request);
        self.list_reviews_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn submit_for_review(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SubmitForReviewRequest>,
    ) -> GrpcResult<pb::SubmitForReviewResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.submit_for_review_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn vote(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::VoteRequest>,
    ) -> GrpcResult<pb::VoteResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.vote_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn comment(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CommentRequest>,
    ) -> GrpcResult<pb::CommentResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.comment_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn reply_thread(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReplyThreadRequest>,
    ) -> GrpcResult<pb::ReplyThreadResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.reply_thread_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn resolve_thread(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveThreadRequest>,
    ) -> GrpcResult<pb::ResolveThreadResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.resolve_thread_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn set_stack(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SetStackRequest>,
    ) -> GrpcResult<pb::SetStackResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.set_stack_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn evaluate_submit(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::EvaluateSubmitRequest>,
    ) -> GrpcResult<pb::EvaluateSubmitResponse> {
        let request = Call::new(ctx, request);
        self.evaluate_submit_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn abandon(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::AbandonRequest>,
    ) -> GrpcResult<pb::AbandonResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.abandon_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Evaluate the submit policy for the caller and, if it permits,
    /// queue the change's current version for landing.
    async fn request_land(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RequestLandRequest>,
    ) -> GrpcResult<pb::RequestLandResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.request_land_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Queue a stack as one ordered group: every change from the lowest
    /// unlanded one up to the given change.
    async fn request_land_stack(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RequestLandStackRequest>,
    ) -> GrpcResult<pb::RequestLandStackResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.request_land_stack_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn get_land_status(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetLandStatusRequest>,
    ) -> GrpcResult<pb::GetLandStatusResponse> {
        let request = Call::new(ctx, request);
        self.get_land_status_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn watch_review(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchReviewRequest>,
    ) -> GrpcResult<ReviewEventStream> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_review_impl(request).await?))
    }
}
