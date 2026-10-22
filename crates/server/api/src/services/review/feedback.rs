//! Votes and comment threads on a review.

use rill_proto::convert::{comment_category_from_proto, confidence_from_proto};
use rill_proto::rill::v1 as pb;
use rill_review::threads::CommentMeta;
use rill_types::{ChangeId, Principal, ThreadAnchor};
use tonic::Request;

use super::owners;
use super::reviews::{change_id, review_response};
use super::ReviewSvc;
use crate::services::common::{self, open, Opened};
use rill_runtime::error::{Error, Result};

async fn thread_of(opened: &Opened, change_id: &ChangeId, thread_id: &str) -> Result<pb::Thread> {
    let state = opened.repo.review().get(change_id).await?;
    state
        .threads
        .iter()
        .find(|t| t.id == thread_id)
        .map(pb::Thread::from)
        .ok_or_else(|| Error::not_found(format!("thread {thread_id}")))
}

impl ReviewSvc {
    pub(super) async fn vote_impl(
        &self,
        request: Request<pb::VoteRequest>,
    ) -> Result<pb::VoteResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().review().await?;
        let req = request.get_ref();
        let id = change_id(&req.change_id)?;
        let value = i8::try_from(req.value)
            .map_err(|_| Error::invalid(format!("vote value {} out of range", req.value)))?;
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
        Ok(pb::VoteResponse {
            review: Some(review_response(state)),
        })
    }

    pub(super) async fn comment_impl(
        &self,
        request: Request<pb::CommentRequest>,
    ) -> Result<pb::CommentResponse> {
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
        Ok(pb::CommentResponse {
            thread_id,
            thread: Some(thread),
        })
    }

    pub(super) async fn reply_thread_impl(
        &self,
        request: Request<pb::ReplyThreadRequest>,
    ) -> Result<pb::ReplyThreadResponse> {
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
        Ok(pb::ReplyThreadResponse {
            thread: Some(thread),
        })
    }

    pub(super) async fn resolve_thread_impl(
        &self,
        request: Request<pb::ResolveThreadRequest>,
    ) -> Result<pb::ResolveThreadResponse> {
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
        Ok(pb::ResolveThreadResponse {
            thread: Some(thread),
        })
    }
}
