//! Votes and comment threads on a review.

use rill_proto::convert::{comment_category_from_proto, confidence_from_proto};
use rill_proto::rill::v1 as pb;
use rill_review::threads::CommentMeta;
use rill_types::{ChangeId, Principal, ThreadAnchor};

use super::owners;
use super::reviews::{review_proto, thread_proto};
use super::ReviewSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

async fn thread_of(opened: &Opened, change_id: &ChangeId, thread_id: &str) -> Result<pb::Thread> {
    let state = opened.repo.review().get(change_id).await?;
    state
        .threads
        .iter()
        .find(|t| t.id == thread_id)
        .map(|t| thread_proto(&opened.repo, change_id, t))
        .ok_or_else(|| Error::not_found(format!("thread {thread_id}")))
}

impl ReviewSvc {
    pub(super) async fn vote_impl(
        &self,
        request: Call<pb::VoteRequest>,
    ) -> Result<pb::VoteResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.review).await?;
        opened.permit().review().await?;
        let id = names::review_in(&opened.repo, &req.review)?;
        let value = i8::try_from(req.value)
            .map_err(|_| Error::invalid(format!("vote value {} out of range", req.value)))?;
        if req.label == owners::OWNER_LABEL && value > 0 {
            owners::check_approval(&opened, &id).await?;
        }
        if !req.version.is_empty() {
            let version = names::version_in(&opened.repo, &req.version)?;
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
        Ok(pb::VoteResponse {
            review: review_proto(&opened.repo, state).into(),
        })
    }

    pub(super) async fn create_thread_impl(
        &self,
        request: Call<pb::CreateThreadRequest>,
    ) -> Result<pb::CreateThreadResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().review().await?;
        let req = request.into_inner();
        let id = names::review_in(&opened.repo, &req.parent)?;
        let anchor = match req.anchor.into_option() {
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
        Ok(pb::CreateThreadResponse {
            thread: thread.into(),
        })
    }

    pub(super) async fn reply_thread_impl(
        &self,
        request: Call<pb::ReplyThreadRequest>,
    ) -> Result<pb::ReplyThreadResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        opened.permit().review().await?;
        let req = request.into_inner();
        let (id, thread_id) = names::thread_in(&opened.repo, &req.name)?;
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
            .reply(&id, &thread_id, opened.principal(), req.body, meta)
            .await?;
        let thread = thread_of(&opened, &id, &thread_id).await?;
        Ok(pb::ReplyThreadResponse {
            thread: thread.into(),
        })
    }

    pub(super) async fn resolve_thread_impl(
        &self,
        request: Call<pb::ResolveThreadRequest>,
    ) -> Result<pb::ResolveThreadResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().review().await?;
        let (id, thread_id) = names::thread_in(&opened.repo, &req.name)?;
        let review = opened.repo.review();
        if req.resolved {
            review.resolve(&id, &thread_id, opened.principal()).await?;
        } else {
            review.unresolve(&id, &thread_id).await?;
        }
        let thread = thread_of(&opened, &id, &thread_id).await?;
        Ok(pb::ResolveThreadResponse {
            thread: thread.into(),
        })
    }
}
