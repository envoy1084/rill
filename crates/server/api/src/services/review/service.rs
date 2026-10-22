//! The `ReviewService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::review_service_server::ReviewService;
use tonic::{Request, Response, Status};

use super::ReviewEventStream;
use crate::services::common::GrpcResult;
use crate::services::idem;
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

#[tonic::async_trait]
impl ReviewService for ReviewSvc {
    async fn get_review(
        &self,
        request: Request<pb::GetReviewRequest>,
    ) -> GrpcResult<pb::GetReviewResponse> {
        self.get_review_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn list_reviews(
        &self,
        request: Request<pb::ListReviewsRequest>,
    ) -> GrpcResult<pb::ListReviewsResponse> {
        self.list_reviews_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn submit_for_review(
        &self,
        request: Request<pb::SubmitForReviewRequest>,
    ) -> GrpcResult<pb::SubmitForReviewResponse> {
        idem::once(&self.state, request, |r| async move {
            self.submit_for_review_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn vote(&self, request: Request<pb::VoteRequest>) -> GrpcResult<pb::VoteResponse> {
        idem::once(&self.state, request, |r| async move {
            self.vote_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn comment(
        &self,
        request: Request<pb::CommentRequest>,
    ) -> GrpcResult<pb::CommentResponse> {
        idem::once(&self.state, request, |r| async move {
            self.comment_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn reply_thread(
        &self,
        request: Request<pb::ReplyThreadRequest>,
    ) -> GrpcResult<pb::ReplyThreadResponse> {
        idem::once(&self.state, request, |r| async move {
            self.reply_thread_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn resolve_thread(
        &self,
        request: Request<pb::ResolveThreadRequest>,
    ) -> GrpcResult<pb::ResolveThreadResponse> {
        idem::once(&self.state, request, |r| async move {
            self.resolve_thread_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn set_stack(
        &self,
        request: Request<pb::SetStackRequest>,
    ) -> GrpcResult<pb::SetStackResponse> {
        idem::once(&self.state, request, |r| async move {
            self.set_stack_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn evaluate_submit(
        &self,
        request: Request<pb::EvaluateSubmitRequest>,
    ) -> GrpcResult<pb::EvaluateSubmitResponse> {
        self.evaluate_submit_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn abandon(
        &self,
        request: Request<pb::AbandonRequest>,
    ) -> GrpcResult<pb::AbandonResponse> {
        idem::once(&self.state, request, |r| async move {
            self.abandon_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Evaluate the submit policy for the caller and, if it permits,
    /// queue the change's current version for landing.
    async fn request_land(
        &self,
        request: Request<pb::RequestLandRequest>,
    ) -> GrpcResult<pb::RequestLandResponse> {
        idem::once(&self.state, request, |r| async move {
            self.request_land_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Queue a stack as one ordered group: every change from the lowest
    /// unlanded one up to the given change.
    async fn request_land_stack(
        &self,
        request: Request<pb::RequestLandStackRequest>,
    ) -> GrpcResult<pb::RequestLandStackResponse> {
        idem::once(&self.state, request, |r| async move {
            self.request_land_stack_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn get_land_status(
        &self,
        request: Request<pb::GetLandStatusRequest>,
    ) -> GrpcResult<pb::GetLandStatusResponse> {
        self.get_land_status_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    type WatchReviewStream = ReviewEventStream;

    async fn watch_review(
        &self,
        request: Request<pb::WatchReviewRequest>,
    ) -> GrpcResult<Self::WatchReviewStream> {
        Ok(Response::new(self.watch_review_impl(request).await?))
    }
}
