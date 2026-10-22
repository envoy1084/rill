//! Reading, listing, submitting, stacking, evaluating and abandoning
//! reviews.

use rill_proto::convert::{
    id_from_proto, ids_from_proto, opt_id_from_proto, review_status_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, RepoPathBuf, ReviewStatus, ThreadAnchor};
use tonic::Request;

use super::ReviewSvc;
use crate::services::common::{self, open};
use rill_runtime::error::Result;

pub(super) fn change_id(bytes: &[u8]) -> Result<ChangeId> {
    Ok(id_from_proto("change_id", bytes)?)
}

pub(super) fn review_response(state: rill_types::ReviewState) -> pb::ReviewState {
    pb::ReviewState::from(state)
}

impl ReviewSvc {
    pub(super) async fn get_review_impl(
        &self,
        request: Request<pb::GetReviewRequest>,
    ) -> Result<pb::GetReviewResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = change_id(&request.get_ref().change_id)?;
        let state = opened.repo.review().get(&id).await?;
        Ok(pb::GetReviewResponse {
            review: Some(review_response(state)),
            labels: opened.repo.labels.iter().map(pb::Label::from).collect(),
        })
    }

    pub(super) async fn list_reviews_impl(
        &self,
        request: Request<pb::ListReviewsRequest>,
    ) -> Result<pb::ListReviewsResponse> {
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
        Ok(pb::ListReviewsResponse {
            reviews: reviews.into_iter().map(review_response).collect(),
            next_page_token: None,
        })
    }

    pub(super) async fn submit_for_review_impl(
        &self,
        request: Request<pb::SubmitForReviewRequest>,
    ) -> Result<pb::SubmitForReviewResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().write_any().await?;
        let id = change_id(&request.get_ref().change_id)?;
        let state = opened.repo.review().submit(&id, opened.principal()).await?;
        Ok(pb::SubmitForReviewResponse {
            review: Some(review_response(state)),
        })
    }

    pub(super) async fn set_stack_impl(
        &self,
        request: Request<pb::SetStackRequest>,
    ) -> Result<pb::SetStackResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().write_any().await?;
        let ids: Vec<ChangeId> = ids_from_proto("change_ids", &request.get_ref().change_ids)?;
        let states = opened.repo.review().set_stack(&ids).await?;
        Ok(pb::SetStackResponse {
            reviews: states.into_iter().map(review_response).collect(),
        })
    }

    pub(super) async fn evaluate_submit_impl(
        &self,
        request: Request<pb::EvaluateSubmitRequest>,
    ) -> Result<pb::EvaluateSubmitResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = change_id(&request.get_ref().change_id)?;
        let review = opened.repo.review();
        let state = review.get(&id).await?;
        let decision = review.evaluate_for(&state, opened.principal()).await?;
        Ok(pb::EvaluateSubmitResponse {
            decision: Some(pb::SubmitDecision {
                allowed: decision.allowed,
                reasons: decision.reasons,
                matched_policies: decision.matched_policies,
                risk: pb::RiskClass::from(state.risk) as i32,
                risk_reasons: state.risk_reasons,
            }),
        })
    }

    pub(super) async fn abandon_impl(
        &self,
        request: Request<pb::AbandonRequest>,
    ) -> Result<pb::AbandonResponse> {
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
        Ok(pb::AbandonResponse {
            review: Some(review_response(state)),
        })
    }
}
