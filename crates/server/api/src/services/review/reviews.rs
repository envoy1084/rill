//! Reading, listing, submitting, stacking, evaluating and abandoning
//! reviews.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, RepoPathBuf, ReviewState, ReviewStatus, ThreadAnchor};

use super::ReviewSvc;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// The wire form of a review, with its name and its threads' names.
pub(super) fn review_proto(ctx: &RepoContext, state: ReviewState) -> pb::ReviewState {
    let id = state.change_id;
    let mut review = pb::ReviewState::from(state);
    review.name = names::review(ctx, &id);
    for thread in &mut review.threads {
        thread.name = names::thread(ctx, &id, &thread.id);
    }
    review
}

/// The wire form of one thread of `change`.
pub(super) fn thread_proto(
    ctx: &RepoContext,
    change: &ChangeId,
    thread: &rill_types::Thread,
) -> pb::Thread {
    pb::Thread {
        name: names::thread(ctx, change, &thread.id),
        ..pb::Thread::from(thread)
    }
}

/// How a status reads in filters.
pub(super) fn status_str(status: ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::Open => "open",
        ReviewStatus::Landable => "landable",
        ReviewStatus::Landing => "landing",
        ReviewStatus::Landed => "landed",
        ReviewStatus::Abandoned => "abandoned",
    }
}

impl ReviewSvc {
    pub(super) async fn get_review_impl(
        &self,
        request: Call<pb::GetReviewRequest>,
    ) -> Result<pb::GetReviewResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = names::review_in(&opened.repo, name)?;
        let state = opened.repo.review().get(&id).await?;
        Ok(pb::GetReviewResponse {
            review: review_proto(&opened.repo, state).into(),
            labels: opened.repo.labels.iter().map(pb::Label::from).collect(),
        })
    }

    pub(super) async fn batch_get_reviews_impl(
        &self,
        request: Call<pb::BatchGetReviewsRequest>,
    ) -> Result<pb::BatchGetReviewsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        if req.names.len() > crate::listing::page::MAX_PAGE_SIZE {
            return Err(Error::invalid("at most 1000 names per batch"));
        }
        let review = opened.repo.review();
        let mut reviews = Vec::with_capacity(req.names.len());
        for name in &req.names {
            let id = names::review_in(&opened.repo, name)?;
            reviews.push(review_proto(&opened.repo, review.get(&id).await?));
        }
        Ok(pb::BatchGetReviewsResponse { reviews })
    }

    pub(super) async fn list_reviews_impl(
        &self,
        request: Call<pb::ListReviewsRequest>,
    ) -> Result<pb::ListReviewsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let filter = Filter::parse(
            &req.filter,
            &[
                ("status", Kind::Str),
                ("owner", Kind::Str),
                ("intent", Kind::Str),
            ],
        )?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut reviews = opened.repo.review().list().await?;
        reviews.retain(|r| filter.matches(&Listed(r)));
        reviews.sort_by_key(|r| (std::cmp::Reverse(r.created_at), r.change_id));
        let (reviews, next_page_token) = page.of(reviews)?;
        Ok(pb::ListReviewsResponse {
            reviews: reviews
                .into_iter()
                .map(|r| review_proto(&opened.repo, r))
                .collect(),
            next_page_token,
        })
    }

    pub(super) async fn submit_change_impl(
        &self,
        request: Call<pb::SubmitChangeRequest>,
    ) -> Result<pb::SubmitChangeResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().write_any().await?;
        let state = opened.repo.review().submit(&id, opened.principal()).await?;
        Ok(pb::SubmitChangeResponse {
            review: review_proto(&opened.repo, state).into(),
        })
    }

    pub(super) async fn update_stack_impl(
        &self,
        request: Call<pb::UpdateStackRequest>,
    ) -> Result<pb::UpdateStackResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().write_any().await?;
        let ids = req
            .changes
            .iter()
            .map(|c| names::change_in(&opened.repo, c))
            .collect::<Result<Vec<_>>>()?;
        let states = opened.repo.review().set_stack(&ids).await?;
        Ok(pb::UpdateStackResponse {
            reviews: states
                .into_iter()
                .map(|r| review_proto(&opened.repo, r))
                .collect(),
        })
    }

    pub(super) async fn evaluate_landing_impl(
        &self,
        request: Call<pb::EvaluateLandingRequest>,
    ) -> Result<pb::EvaluateLandingResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let review = opened.repo.review();
        let state = review.get(&id).await?;
        let decision = review.evaluate_for(&state, opened.principal()).await?;
        Ok(pb::EvaluateLandingResponse {
            decision: (pb::SubmitDecision {
                allowed: decision.allowed,
                reasons: decision.reasons,
                matched_policies: decision.matched_policies,
                risk: pb::RiskClass::from(state.risk).into(),
                risk_reasons: state.risk_reasons,
            })
            .into(),
        })
    }

    pub(super) async fn abandon_change_impl(
        &self,
        request: Call<pb::AbandonChangeRequest>,
    ) -> Result<pb::AbandonChangeResponse> {
        let (opened, id) = self.change(&request, &request.get_ref().change).await?;
        opened.permit().write_any().await?;
        let review = opened.repo.review();
        let current = review.get(&id).await?;
        opened.permit().owns_or_admin(&current.owner).await?;
        let state = review.abandon(&id, opened.principal()).await?;
        if let Some(reason) = request.get_ref().reason.clone().filter(|r| !r.is_empty()) {
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
        Ok(pb::AbandonChangeResponse {
            review: review_proto(&opened.repo, state).into(),
        })
    }

    /// Open the repository a change name is in, and the change's id.
    pub(super) async fn change<T>(
        &self,
        request: &Call<T>,
        name: &str,
    ) -> Result<(Opened, ChangeId)> {
        let opened = open_repo(&self.state, request, name).await?;
        let id = names::change_in(&opened.repo, name)?;
        Ok((opened, id))
    }
}

/// A review as a list filter sees it.
struct Listed<'a>(&'a ReviewState);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "status" => FieldValue::Str(status_str(self.0.status).into()),
            "owner" => FieldValue::Str(self.0.owner.to_string()),
            "intent" => match self.0.intent {
                Some(i) => FieldValue::Str(i.to_string()),
                None => FieldValue::Missing,
            },
            _ => FieldValue::Missing,
        }
    }
}
