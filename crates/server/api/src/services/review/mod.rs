//! `rill.v1.ReviewService`: a thin mapping onto `rill_review::ReviewService`
//! plus the hand-off to the lander.

mod owners;
mod watch;

use std::sync::Arc;

use rill_lander::LandRequest;
use rill_proto::convert::{
    comment_category_from_proto, confidence_from_proto, hash_to_proto, id_from_proto,
    ids_from_proto, opt_id_from_proto, review_status_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::review_service_server::ReviewService;
use rill_review::threads::CommentMeta;
use rill_types::{ChangeId, Principal, RepoPathBuf, ReviewStatus, ThreadAnchor};
use tonic::{Request, Response, Status};

use super::common::{self, open, GrpcResult, Opened};
use super::idem;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Server stream of review events.
pub type ReviewEventStream = super::repo::EventStream<pb::ReviewEvent>;

/// The review service.
#[derive(Clone)]
pub struct ReviewSvc {
    state: Arc<AppState>,
}

impl ReviewSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ReviewSvc { state }
    }
}

fn change_id(bytes: &[u8]) -> Result<ChangeId> {
    Ok(id_from_proto("change_id", bytes)?)
}

fn review_response(state: rill_types::ReviewState) -> pb::ReviewState {
    pb::ReviewState::from(state)
}

async fn thread_of(opened: &Opened, change_id: &ChangeId, thread_id: &str) -> Result<pb::Thread> {
    let state = opened.repo.review().get(change_id).await?;
    state
        .threads
        .iter()
        .find(|t| t.id == thread_id)
        .map(pb::Thread::from)
        .ok_or_else(|| Error::not_found(format!("thread {thread_id}")))
}

#[tonic::async_trait]
impl ReviewService for ReviewSvc {
    async fn get_review(
        &self,
        request: Request<pb::GetReviewRequest>,
    ) -> GrpcResult<pb::GetReviewResponse> {
        let result: rill_runtime::error::Result<Response<pb::GetReviewResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let id = change_id(&request.get_ref().change_id)?;
            let state = opened.repo.review().get(&id).await?;
            Ok(Response::new(pb::GetReviewResponse {
                review: Some(review_response(state)),
                labels: opened.repo.labels.iter().map(pb::Label::from).collect(),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn list_reviews(
        &self,
        request: Request<pb::ListReviewsRequest>,
    ) -> GrpcResult<pb::ListReviewsResponse> {
        let result: rill_runtime::error::Result<Response<pb::ListReviewsResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let statuses: Vec<ReviewStatus> = req
                .status
                .iter()
                .map(|s| review_status_from_proto("status", *s))
                .collect::<std::result::Result<_, _>>()?;
            let owner: Option<rill_types::PrincipalId> =
                opt_id_from_proto("owner", req.owner.as_ref())?;
            let intent: Option<rill_types::IntentId> =
                opt_id_from_proto("intent", req.intent.as_ref())?;
            let limit = common::limit(req.limit, 100, 10_000);
            let mut reviews = opened.repo.review().list().await?;
            reviews.retain(|r| {
                (statuses.is_empty() || statuses.contains(&r.status))
                    && owner.is_none_or(|o| r.owner.id == o)
                    && intent.is_none_or(|i| r.intent == Some(i))
            });
            reviews.sort_by_key(|r| std::cmp::Reverse(r.updated_at));
            reviews.truncate(limit);
            Ok(Response::new(pb::ListReviewsResponse {
                reviews: reviews.into_iter().map(review_response).collect(),
                next_page_token: None,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn submit_for_review(
        &self,
        request: Request<pb::SubmitForReviewRequest>,
    ) -> GrpcResult<pb::SubmitForReviewResponse> {
        let result: rill_runtime::error::Result<Response<pb::SubmitForReviewResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().write_any().await?;
                let id = change_id(&request.get_ref().change_id)?;
                let state = opened.repo.review().submit(&id, opened.principal()).await?;
                Ok(Response::new(pb::SubmitForReviewResponse {
                    review: Some(review_response(state)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn vote(&self, request: Request<pb::VoteRequest>) -> GrpcResult<pb::VoteResponse> {
        let result: rill_runtime::error::Result<Response<pb::VoteResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().review().await?;
                let req = request.get_ref();
                let id = change_id(&req.change_id)?;
                let value = i8::try_from(req.value).map_err(|_| {
                    Error::invalid(format!("vote value {} out of range", req.value))
                })?;
                if req.label == owners::OWNER_LABEL && value > 0 {
                    owners::check_approval(&opened, &id).await?;
                }
                if let Some(version) = common::opt_hash("version", req.version.as_ref())? {
                    let current = opened.repo.review().get(&id).await?.current_version();
                    if current != Some(version) {
                        return Err(Error::precondition(format!(
                            "version {version} is not the current version of {id}"
                        )));
                    }
                }
                let state = opened
                    .repo
                    .review()
                    .vote(
                        &id,
                        opened.principal(),
                        &req.label,
                        value,
                        req.reason.clone(),
                    )
                    .await?;
                Ok(Response::new(pb::VoteResponse {
                    review: Some(review_response(state)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn comment(
        &self,
        request: Request<pb::CommentRequest>,
    ) -> GrpcResult<pb::CommentResponse> {
        let result: rill_runtime::error::Result<Response<pb::CommentResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().review().await?;
                let req = request.into_inner();
                let id = change_id(&req.change_id)?;
                let anchor = match req.anchor {
                    Some(a) => ThreadAnchor::try_from(a)?,
                    None => ThreadAnchor::Change,
                };
                let category = req
                    .category
                    .map(|c| comment_category_from_proto("category", c))
                    .transpose()?;
                let confidence = req
                    .confidence
                    .map(|c| confidence_from_proto("confidence", c))
                    .transpose()?;
                let to = req.to.map(Principal::try_from).transpose()?;
                let thread_id = opened
                    .repo
                    .review()
                    .comment(
                        &id,
                        opened.principal(),
                        anchor,
                        req.body,
                        category,
                        confidence,
                        to,
                    )
                    .await?;
                let thread = thread_of(&opened, &id, &thread_id).await?;
                Ok(Response::new(pb::CommentResponse {
                    thread_id,
                    thread: Some(thread),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn reply_thread(
        &self,
        request: Request<pb::ReplyThreadRequest>,
    ) -> GrpcResult<pb::ReplyThreadResponse> {
        let result: rill_runtime::error::Result<Response<pb::ReplyThreadResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().review().await?;
                let req = request.into_inner();
                let id = change_id(&req.change_id)?;
                let meta = CommentMeta {
                    category: req
                        .category
                        .map(|c| comment_category_from_proto("category", c))
                        .transpose()?,
                    confidence: req
                        .confidence
                        .map(|c| confidence_from_proto("confidence", c))
                        .transpose()?,
                };
                opened
                    .repo
                    .review()
                    .reply(&id, &req.thread_id, opened.principal(), req.body, meta)
                    .await?;
                let thread = thread_of(&opened, &id, &req.thread_id).await?;
                Ok(Response::new(pb::ReplyThreadResponse {
                    thread: Some(thread),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn resolve_thread(
        &self,
        request: Request<pb::ResolveThreadRequest>,
    ) -> GrpcResult<pb::ResolveThreadResponse> {
        let result: rill_runtime::error::Result<Response<pb::ResolveThreadResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().review().await?;
                let req = request.get_ref();
                let id = change_id(&req.change_id)?;
                let review = opened.repo.review();
                if req.resolved {
                    review
                        .resolve(&id, &req.thread_id, opened.principal())
                        .await?;
                } else {
                    review.unresolve(&id, &req.thread_id).await?;
                }
                let thread = thread_of(&opened, &id, &req.thread_id).await?;
                Ok(Response::new(pb::ResolveThreadResponse {
                    thread: Some(thread),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn set_stack(
        &self,
        request: Request<pb::SetStackRequest>,
    ) -> GrpcResult<pb::SetStackResponse> {
        let result: rill_runtime::error::Result<Response<pb::SetStackResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().write_any().await?;
                let ids: Vec<ChangeId> =
                    ids_from_proto("change_ids", &request.get_ref().change_ids)?;
                let states = opened.repo.review().set_stack(&ids).await?;
                Ok(Response::new(pb::SetStackResponse {
                    reviews: states.into_iter().map(review_response).collect(),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn evaluate_submit(
        &self,
        request: Request<pb::EvaluateSubmitRequest>,
    ) -> GrpcResult<pb::EvaluateSubmitResponse> {
        let result: rill_runtime::error::Result<Response<pb::EvaluateSubmitResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let id = change_id(&request.get_ref().change_id)?;
            let review = opened.repo.review();
            let state = review.get(&id).await?;
            let decision = review.evaluate_for(&state, opened.principal()).await?;
            Ok(Response::new(pb::EvaluateSubmitResponse {
                decision: Some(pb::SubmitDecision {
                    allowed: decision.allowed,
                    reasons: decision.reasons,
                    matched_policies: decision.matched_policies,
                    risk: pb::RiskClass::from(state.risk) as i32,
                    risk_reasons: state.risk_reasons,
                }),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn abandon(
        &self,
        request: Request<pb::AbandonRequest>,
    ) -> GrpcResult<pb::AbandonResponse> {
        let result: rill_runtime::error::Result<Response<pb::AbandonResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().write_any().await?;
                let req = request.get_ref();
                let id = change_id(&req.change_id)?;
                let review = opened.repo.review();
                let current = review.get(&id).await?;
                opened.permit().owns_or_admin(&current.owner).await?;
                let state = review.abandon(&id, opened.principal()).await?;
                if let Some(reason) = req.reason.clone().filter(|r| !r.is_empty()) {
                    review
                        .comment(
                            &id,
                            opened.principal(),
                            ThreadAnchor::Change,
                            format!("Abandoned: {reason}"),
                            None,
                            None,
                            None,
                        )
                        .await?;
                }
                Ok(Response::new(pb::AbandonResponse {
                    review: Some(review_response(state)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    /// Evaluate the submit policy for the caller and, if it permits,
    /// queue the change's current version for landing.
    async fn request_land(
        &self,
        request: Request<pb::RequestLandRequest>,
    ) -> GrpcResult<pb::RequestLandResponse> {
        let result: rill_runtime::error::Result<Response<pb::RequestLandResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().land().await?;
                let id = change_id(&request.get_ref().change_id)?;
                let review = opened.repo.review();
                let state = review.get(&id).await?;
                match state.status {
                    ReviewStatus::Landed => {
                        return Err(Error::precondition(format!("change {id} already landed")))
                    }
                    ReviewStatus::Landing => {
                        return Err(Error::precondition(format!(
                            "change {id} is already landing"
                        )))
                    }
                    ReviewStatus::Abandoned => {
                        return Err(Error::precondition(format!("change {id} is abandoned")))
                    }
                    _ => {}
                }
                let decision = review.evaluate_for(&state, opened.principal()).await?;
                if !decision.allowed {
                    return Err(Error::precondition(format!(
                        "submit policy does not allow landing: {}",
                        decision.reasons.join("; ")
                    )));
                }
                let version = state
                    .current_version()
                    .ok_or_else(|| Error::precondition(format!("change {id} has no versions")))?;
                crate::services::budget::spend_now(&opened, crate::services::budget::LANDS).await?;
                let ticket = opened
                    .repo
                    .lander
                    .enqueue(
                        LandRequest::new(id, version, opened.principal().clone(), state.risk)
                            .sponsored_by(
                                opened.ctx.claims.as_ref().and_then(|c| c.sponsor.clone()),
                            ),
                    )
                    .await?;
                let (proto, _) = common::ticket_status(&opened.repo, ticket).await?;
                Ok(Response::new(pb::RequestLandResponse {
                    ticket: Some(proto),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn get_land_status(
        &self,
        request: Request<pb::GetLandStatusRequest>,
    ) -> GrpcResult<pb::GetLandStatusResponse> {
        let result: rill_runtime::error::Result<Response<pb::GetLandStatusResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let ticket = common::parse_ticket(&request.get_ref().ticket)?;
            let (proto, status) = common::ticket_status(&opened.repo, ticket).await?;
            let (seq, landed_hash, error) = match status {
                rill_lander::LandStatus::Landed { seq } => {
                    let hash = opened.repo.repo.trunk_at(seq).await?;
                    (Some(seq.get()), hash.map(|h| hash_to_proto(&h)), None)
                }
                rill_lander::LandStatus::Failed { reason }
                | rill_lander::LandStatus::Returned { reason }
                | rill_lander::LandStatus::Ejected { reason } => (None, None, Some(reason)),
                rill_lander::LandStatus::Conflict { paths } => (
                    None,
                    None,
                    Some(format!(
                        "rebase onto trunk conflicts at: {}",
                        paths
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ),
                _ => (None, None, None),
            };
            let place = if status_is_queued(&proto) {
                opened.repo.lander.position(ticket).await?
            } else {
                None
            };
            Ok(Response::new(pb::GetLandStatusResponse {
                ticket: Some(proto),
                seq,
                landed_hash,
                error,
                position: place.map(|(at, _)| at as u32),
                eta_ms: place.map(|(_, wait)| wait.as_millis() as u64),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    type WatchReviewStream = ReviewEventStream;

    async fn watch_review(
        &self,
        request: Request<pb::WatchReviewRequest>,
    ) -> GrpcResult<Self::WatchReviewStream> {
        Ok(Response::new(self.watch_review_impl(request).await?))
    }
}

fn status_is_queued(ticket: &pb::LandTicket) -> bool {
    ticket.status == pb::LandStatus::Queued as i32
}
