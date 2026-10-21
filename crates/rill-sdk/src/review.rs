//! Reviews: state, votes, threads, stacks, submit evaluation and landing.

use rill_proto::convert::{hash_to_proto, id_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeVersionHash, Label, OperationId, ReviewState, Thread};

use crate::client::Client;
use crate::error::Result;
use crate::repo::watch_stream;
use crate::stream::EventStream;
use crate::types::{
    LandProgress, LandTicket, NewComment, NewThread, Page, Reply, ReviewEvent, ReviewFilter,
    ReviewInfo, SubmitDecision,
};

/// Review operations on one repository.
#[derive(Clone, Debug)]
pub struct ReviewClient {
    client: Client,
    repo: String,
}

impl ReviewClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        ReviewClient { client, repo }
    }

    fn svc(&self) -> crate::client::ReviewSvc {
        self.client.review_svc()
    }

    /// The review state of a change with the repository's labels.
    pub async fn get(&self, change_id: ChangeId) -> Result<ReviewInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::GetReviewRequest {
                repo: self.repo.clone(),
                change_id: id_to_proto(&change_id),
            };
            async move { svc.get_review(request).await }
        })
        .await?
        .into_inner();
        Ok(ReviewInfo {
            review: required("get_review.review", resp.review)?.try_into()?,
            labels: resp
                .labels
                .into_iter()
                .map(Label::try_from)
                .collect::<std::result::Result<_, _>>()?,
        })
    }

    /// Reviews matching a filter.
    pub async fn list(&self, filter: ReviewFilter) -> Result<Page<ReviewState>> {
        let request = pb::ListReviewsRequest {
            repo: self.repo.clone(),
            status: filter
                .status
                .into_iter()
                .map(|s| pb::ReviewStatus::from(s) as i32)
                .collect(),
            owner: filter.owner.as_ref().map(id_to_proto),
            intent: filter.intent.as_ref().map(id_to_proto),
            limit: filter.limit,
            page_token: filter.page_token,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.list_reviews(request).await }
        })
        .await?
        .into_inner();
        let items = convert_states(resp.reviews)?;
        Ok(Page::new(items, resp.next_page_token))
    }

    /// Submit a change for review.
    pub async fn submit(&self, change_id: ChangeId) -> Result<ReviewState> {
        self.submit_with_op(OperationId::new(), change_id).await
    }

    /// [`ReviewClient::submit`] with a caller-chosen idempotency key.
    pub async fn submit_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
    ) -> Result<ReviewState> {
        let request = pb::SubmitForReviewRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.submit_for_review(request).await }
        })
        .await?
        .into_inner();
        Ok(required("submit_for_review.review", resp.review)?.try_into()?)
    }

    /// Cast a vote on a label, for the current version when `version` is
    /// `None`.
    pub async fn vote(
        &self,
        change_id: ChangeId,
        label: &str,
        value: i8,
        reason: Option<String>,
        version: Option<ChangeVersionHash>,
    ) -> Result<ReviewState> {
        self.vote_with_op(OperationId::new(), change_id, label, value, reason, version)
            .await
    }

    /// [`ReviewClient::vote`] with a caller-chosen idempotency key.
    pub async fn vote_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        label: &str,
        value: i8,
        reason: Option<String>,
        version: Option<ChangeVersionHash>,
    ) -> Result<ReviewState> {
        let request = pb::VoteRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
            label: label.to_string(),
            value: value as i32,
            reason,
            version: version.as_ref().map(hash_to_proto),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.vote(request).await }
        })
        .await?
        .into_inner();
        Ok(required("vote.review", resp.review)?.try_into()?)
    }

    /// Open a thread with a first comment.
    pub async fn comment(&self, change_id: ChangeId, comment: NewComment) -> Result<NewThread> {
        self.comment_with_op(OperationId::new(), change_id, comment)
            .await
    }

    /// [`ReviewClient::comment`] with a caller-chosen idempotency key.
    pub async fn comment_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        comment: NewComment,
    ) -> Result<NewThread> {
        let request = pb::CommentRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
            anchor: Some(pb::ThreadAnchor::from(&comment.anchor)),
            body: comment.body,
            category: comment
                .category
                .map(|c| pb::CommentCategory::from(c) as i32),
            confidence: comment.confidence.map(u32::from),
            to: comment.to.map(pb::Principal::from),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.comment(request).await }
        })
        .await?
        .into_inner();
        Ok(NewThread {
            thread_id: resp.thread_id,
            thread: required("comment.thread", resp.thread)?.try_into()?,
        })
    }

    /// Reply on a thread.
    pub async fn reply(
        &self,
        change_id: ChangeId,
        thread_id: &str,
        reply: Reply,
    ) -> Result<Thread> {
        self.reply_with_op(OperationId::new(), change_id, thread_id, reply)
            .await
    }

    /// [`ReviewClient::reply`] with a caller-chosen idempotency key.
    pub async fn reply_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        thread_id: &str,
        reply: Reply,
    ) -> Result<Thread> {
        let request = pb::ReplyThreadRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
            thread_id: thread_id.to_string(),
            body: reply.body,
            category: reply.category.map(|c| pb::CommentCategory::from(c) as i32),
            confidence: reply.confidence.map(u32::from),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.reply_thread(request).await }
        })
        .await?
        .into_inner();
        Ok(required("reply_thread.thread", resp.thread)?.try_into()?)
    }

    /// Resolve (or with `false`, reopen) a thread.
    pub async fn resolve_thread(
        &self,
        change_id: ChangeId,
        thread_id: &str,
        resolved: bool,
    ) -> Result<Thread> {
        self.resolve_thread_with_op(OperationId::new(), change_id, thread_id, resolved)
            .await
    }

    /// [`ReviewClient::resolve_thread`] with a caller-chosen idempotency key.
    pub async fn resolve_thread_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        thread_id: &str,
        resolved: bool,
    ) -> Result<Thread> {
        let request = pb::ResolveThreadRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
            thread_id: thread_id.to_string(),
            resolved,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.resolve_thread(request).await }
        })
        .await?
        .into_inner();
        Ok(required("resolve_thread.thread", resp.thread)?.try_into()?)
    }

    /// Declare a stack, bottom to top. Every change gets the same stack.
    pub async fn set_stack(&self, change_ids: &[ChangeId]) -> Result<Vec<ReviewState>> {
        self.set_stack_with_op(OperationId::new(), change_ids).await
    }

    /// [`ReviewClient::set_stack`] with a caller-chosen idempotency key.
    pub async fn set_stack_with_op(
        &self,
        op: OperationId,
        change_ids: &[ChangeId],
    ) -> Result<Vec<ReviewState>> {
        let request = pb::SetStackRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_ids: change_ids.iter().map(id_to_proto).collect(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.set_stack(request).await }
        })
        .await?
        .into_inner();
        convert_states(resp.reviews)
    }

    /// Evaluate submit rules without landing.
    pub async fn evaluate_submit(&self, change_id: ChangeId) -> Result<SubmitDecision> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::EvaluateSubmitRequest {
                repo: self.repo.clone(),
                change_id: id_to_proto(&change_id),
            };
            async move { svc.evaluate_submit(request).await }
        })
        .await?
        .into_inner();
        required("evaluate_submit.decision", resp.decision)?.try_into()
    }

    /// Abandon a change.
    pub async fn abandon(
        &self,
        change_id: ChangeId,
        reason: Option<String>,
    ) -> Result<ReviewState> {
        self.abandon_with_op(OperationId::new(), change_id, reason)
            .await
    }

    /// [`ReviewClient::abandon`] with a caller-chosen idempotency key.
    pub async fn abandon_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        reason: Option<String>,
    ) -> Result<ReviewState> {
        let request = pb::AbandonRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
            reason,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.abandon(request).await }
        })
        .await?
        .into_inner();
        Ok(required("abandon.review", resp.review)?.try_into()?)
    }

    /// Ask the lander to land a change. Returns the ticket to poll.
    pub async fn request_land(&self, change_id: ChangeId) -> Result<LandTicket> {
        self.request_land_with_op(OperationId::new(), change_id)
            .await
    }

    /// [`ReviewClient::request_land`] with a caller-chosen idempotency key.
    pub async fn request_land_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
    ) -> Result<LandTicket> {
        let request = pb::RequestLandRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            change_id: id_to_proto(&change_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.request_land(request).await }
        })
        .await?
        .into_inner();
        required("request_land.ticket", resp.ticket)?.try_into()
    }

    /// Where a landing is, by ticket id.
    pub async fn land_status(&self, ticket: &str) -> Result<LandProgress> {
        crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::GetLandStatusRequest {
                repo: self.repo.clone(),
                ticket: ticket.to_string(),
            };
            async move { svc.get_land_status(request).await }
        })
        .await?
        .into_inner()
        .try_into()
    }

    /// Events on one change's review.
    pub async fn watch(&self, change_id: ChangeId) -> Result<EventStream<ReviewEvent>> {
        let stream = self
            .svc()
            .watch_review(pb::WatchReviewRequest {
                repo: self.repo.clone(),
                change_id: id_to_proto(&change_id),
            })
            .await?
            .into_inner();
        Ok(watch_stream(stream))
    }
}

fn convert_states(states: Vec<pb::ReviewState>) -> Result<Vec<ReviewState>> {
    states
        .into_iter()
        .map(|s| ReviewState::try_from(s).map_err(Into::into))
        .collect()
}
