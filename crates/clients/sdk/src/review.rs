//! Reviews: state, votes, threads, stacks, submit evaluation and landing.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeVersionHash, Label, OperationId, ReviewState, Thread};

use crate::client::Client;
use crate::error::Result;
use crate::names;
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
    /// The repository's full resource name.
    repo: String,
}

impl ReviewClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        ReviewClient { client, repo }
    }

    fn svc(&self) -> crate::client::ReviewSvc {
        self.client.review_svc()
    }

    fn landing(&self) -> crate::client::LandingSvc {
        self.client.landing_svc()
    }

    fn change(&self, id: &ChangeId) -> String {
        names::change(&self.repo, id)
    }

    fn review(&self, id: &ChangeId) -> String {
        format!("{}/review", self.change(id))
    }

    fn thread(&self, id: &ChangeId, thread: &str) -> String {
        format!("{}/threads/{thread}", self.review(id))
    }

    /// The review state of a change with the repository's labels.
    pub async fn get(&self, change_id: ChangeId) -> Result<ReviewInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetReviewRequest {
                name: self.review(&change_id),
            };
            async move { svc.get_review(request).await }
        })
        .await?
        .into_owned();
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
            parent: self.repo.clone(),
            page_size: filter.limit as i32,
            page_token: filter.page_token.clone().unwrap_or_default(),
            filter: filter.to_filter(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_reviews(request).await }
        })
        .await?
        .into_owned();
        let items = convert_states(resp.reviews)?;
        Ok(Page::new(
            items,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
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
        let request = pb::SubmitChangeRequest {
            request_id: names::request_id(&op),
            change: self.change(&change_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.submit_change(request).await }
        })
        .await?
        .into_owned();
        Ok(required("submit_change.review", resp.review)?.try_into()?)
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
            request_id: names::request_id(&op),
            review: self.review(&change_id),
            label: label.to_string(),
            value: value as i32,
            reason,
            version: version
                .map(|v| names::version(&self.repo, &v))
                .unwrap_or_default(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.vote(request).await }
        })
        .await?
        .into_owned();
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
        let request = pb::CreateThreadRequest {
            request_id: names::request_id(&op),
            parent: self.review(&change_id),
            anchor: pb::ThreadAnchor::from(&comment.anchor).into(),
            body: comment.body,
            category: comment
                .category
                .map(|c| pb::CommentCategory::from(c).into()),
            confidence: comment.confidence.map(u32::from),
            to: comment.to.map(pb::Principal::from).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_thread(request).await }
        })
        .await?
        .into_owned();
        let thread: Thread = required("create_thread.thread", resp.thread)?.try_into()?;
        Ok(NewThread {
            thread_id: thread.id.clone(),
            thread,
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
            request_id: names::request_id(&op),
            name: self.thread(&change_id, thread_id),
            body: reply.body,
            category: reply.category.map(|c| pb::CommentCategory::from(c).into()),
            confidence: reply.confidence.map(u32::from),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.reply_thread(request).await }
        })
        .await?
        .into_owned();
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
            request_id: names::request_id(&op),
            name: self.thread(&change_id, thread_id),
            resolved,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.resolve_thread(request).await }
        })
        .await?
        .into_owned();
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
        let request = pb::UpdateStackRequest {
            request_id: names::request_id(&op),
            parent: self.repo.clone(),
            changes: change_ids.iter().map(|c| self.change(c)).collect(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_stack(request).await }
        })
        .await?
        .into_owned();
        convert_states(resp.reviews)
    }

    /// Evaluate submit rules without landing.
    pub async fn evaluate_submit(&self, change_id: ChangeId) -> Result<SubmitDecision> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::EvaluateLandingRequest {
                change: self.change(&change_id),
            };
            async move { svc.evaluate_landing(request).await }
        })
        .await?
        .into_owned();
        required("evaluate_landing.decision", resp.decision)?.try_into()
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
        let request = pb::AbandonChangeRequest {
            request_id: names::request_id(&op),
            change: self.change(&change_id),
            reason,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.abandon_change(request).await }
        })
        .await?
        .into_owned();
        Ok(required("abandon_change.review", resp.review)?.try_into()?)
    }

    /// Ask the lander to land a change on trunk. Returns the ticket to poll.
    pub async fn request_land(&self, change_id: ChangeId) -> Result<LandTicket> {
        self.request_land_with_op(OperationId::new(), change_id, None)
            .await
    }

    /// Ask the lander of `onto` (`trunk`, or `release/<name>`) to land a
    /// change. Returns the ticket to poll.
    pub async fn request_land_onto(&self, change_id: ChangeId, onto: &str) -> Result<LandTicket> {
        self.request_land_with_op(OperationId::new(), change_id, Some(onto))
            .await
    }

    /// [`ReviewClient::request_land_onto`] with a caller-chosen idempotency
    /// key; `None` lands on trunk.
    pub async fn request_land_with_op(
        &self,
        op: OperationId,
        change_id: ChangeId,
        onto: Option<&str>,
    ) -> Result<LandTicket> {
        let request = pb::RequestLandRequest {
            request_id: names::request_id(&op),
            change: self.change(&change_id),
            onto: onto.unwrap_or_default().to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.landing(), request.clone());
            async move { svc.request_land(request).await }
        })
        .await?
        .into_owned();
        required("request_land.land_request", resp.land_request)?.try_into()
    }

    /// Ask the lander of `onto` (trunk when `None`) to land `change_id`'s
    /// stack, from its lowest unlanded change up to it, as one ordered
    /// group: all or nothing, or with `prefix` as far as it can. Returns the
    /// tickets, bottom first.
    pub async fn request_land_stack(
        &self,
        change_id: ChangeId,
        prefix: bool,
        onto: Option<&str>,
    ) -> Result<Vec<LandTicket>> {
        let request = pb::RequestLandStackRequest {
            request_id: names::request_id(&OperationId::new()),
            change: self.change(&change_id),
            prefix,
            onto: onto.unwrap_or_default().to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.landing(), request.clone());
            async move { svc.request_land_stack(request).await }
        })
        .await?
        .into_owned();
        resp.land_requests
            .into_iter()
            .map(LandTicket::try_from)
            .collect()
    }

    /// Where a landing is, by ticket id or land request name.
    pub async fn land_status(&self, ticket: &str) -> Result<LandProgress> {
        let name = match ticket.contains('/') {
            true => ticket.to_string(),
            false => format!("{}/landRequests/{ticket}", self.repo),
        };
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.landing();
            let request = pb::GetLandRequestRequest { name: name.clone() };
            async move { svc.get_land_request(request).await }
        })
        .await?
        .into_owned();
        required("get_land_request.land_request", resp.land_request)?.try_into()
    }

    /// Events on one change's review.
    pub async fn watch(&self, change_id: ChangeId) -> Result<EventStream<ReviewEvent>> {
        let stream = self
            .svc()
            .watch_review(pb::WatchReviewRequest {
                name: self.review(&change_id),
            })
            .await?;
        Ok(watch_stream(stream))
    }
}

fn convert_states(states: Vec<pb::ReviewState>) -> Result<Vec<ReviewState>> {
    states
        .into_iter()
        .map(|s| ReviewState::try_from(s).map_err(Into::into))
        .collect()
}
