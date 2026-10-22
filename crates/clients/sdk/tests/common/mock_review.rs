//! Review service whose land tickets follow a scripted status sequence.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use bytes::Bytes;
use futures::stream::BoxStream;
use rill_proto::convert::{hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::review_service_server::ReviewService;
use rill_types::{ChangeId, Principal};
use tonic::{Request, Response, Status};

use super::fixtures::fake_hash;
use super::rpc::{unimplemented_rpcs, BoxFut, RpcResult};

/// Review service whose land tickets follow a scripted status sequence.
#[derive(Default)]
pub struct MockReview {
    /// Statuses to report, in order; the last one repeats.
    pub script: Mutex<VecDeque<pb::LandStatus>>,
    pub polls: Mutex<usize>,
    pub submitted: Mutex<Vec<ChangeId>>,
    pub stacks: Mutex<Vec<Vec<ChangeId>>>,
    pub land_requests: Mutex<Vec<ChangeId>>,
}

impl MockReview {
    pub fn with_script(statuses: &[pb::LandStatus]) -> Self {
        MockReview {
            script: Mutex::new(statuses.iter().copied().collect()),
            ..Default::default()
        }
    }

    fn ticket(&self, id: &str, change_id: Bytes, status: pb::LandStatus) -> pb::LandTicket {
        pb::LandTicket {
            id: id.to_string(),
            change_id,
            status: status as i32,
            requested_at: 1,
        }
    }

    fn review_state(change_id: Bytes, stack: Vec<Bytes>) -> pb::ReviewState {
        pb::ReviewState {
            change_id,
            versions: vec![hash_to_proto(&fake_hash("v1"))],
            status: pb::ReviewStatus::Open as i32,
            intent: None,
            owner: Some(pb::Principal::from(Principal::human("alice"))),
            votes: vec![],
            threads: vec![],
            risk: pb::RiskClass::Normal as i32,
            risk_reasons: vec![],
            stack,
            landed_seq: None,
            created_at: 1,
            updated_at: 1,
            extra: BTreeMap::new(),
        }
    }
}

#[tonic::async_trait]
impl ReviewService for MockReview {
    type WatchReviewStream = BoxStream<'static, Result<pb::ReviewEvent, Status>>;

    async fn submit_for_review(
        &self,
        req: Request<pb::SubmitForReviewRequest>,
    ) -> RpcResult<pb::SubmitForReviewResponse> {
        let req = req.into_inner();
        let id: ChangeId = rill_proto::convert::id_from_proto("change_id", &req.change_id)?;
        self.submitted.lock().unwrap().push(id);
        Ok(Response::new(pb::SubmitForReviewResponse {
            review: Some(Self::review_state(req.change_id, vec![])),
        }))
    }

    async fn set_stack(
        &self,
        req: Request<pb::SetStackRequest>,
    ) -> RpcResult<pb::SetStackResponse> {
        let req = req.into_inner();
        let ids: Vec<ChangeId> =
            rill_proto::convert::ids_from_proto("change_ids", &req.change_ids)?;
        self.stacks.lock().unwrap().push(ids);
        Ok(Response::new(pb::SetStackResponse {
            reviews: req
                .change_ids
                .iter()
                .map(|id| Self::review_state(id.clone(), req.change_ids.clone()))
                .collect(),
        }))
    }

    async fn request_land(
        &self,
        req: Request<pb::RequestLandRequest>,
    ) -> RpcResult<pb::RequestLandResponse> {
        let req = req.into_inner();
        let id: ChangeId = rill_proto::convert::id_from_proto("change_id", &req.change_id)?;
        self.land_requests.lock().unwrap().push(id);
        Ok(Response::new(pb::RequestLandResponse {
            ticket: Some(self.ticket(
                &format!("ticket-{id}"),
                req.change_id,
                pb::LandStatus::Queued,
            )),
        }))
    }

    async fn request_land_stack(
        &self,
        req: Request<pb::RequestLandStackRequest>,
    ) -> RpcResult<pb::RequestLandStackResponse> {
        let req = req.into_inner();
        let id: ChangeId = rill_proto::convert::id_from_proto("change_id", &req.change_id)?;
        let stack = self
            .stacks
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|s| s.contains(&id))
            .cloned()
            .unwrap_or_else(|| vec![id]);
        self.land_requests
            .lock()
            .unwrap()
            .extend(stack.iter().copied());
        Ok(Response::new(pb::RequestLandStackResponse {
            tickets: stack
                .iter()
                .map(|c| {
                    self.ticket(
                        &format!("ticket-{c}"),
                        id_to_proto(c),
                        pb::LandStatus::Queued,
                    )
                })
                .collect(),
        }))
    }

    async fn get_land_status(
        &self,
        req: Request<pb::GetLandStatusRequest>,
    ) -> RpcResult<pb::GetLandStatusResponse> {
        let ticket = req.into_inner().ticket;
        *self.polls.lock().unwrap() += 1;
        let status = {
            let mut script = self.script.lock().unwrap();
            if script.len() > 1 {
                script.pop_front().unwrap()
            } else {
                script.front().copied().unwrap_or(pb::LandStatus::Landed)
            }
        };
        let landed = status == pb::LandStatus::Landed;
        let change_id = id_to_proto(&ChangeId::derived("ticket", &ticket));
        Ok(Response::new(pb::GetLandStatusResponse {
            position: None,
            eta_ms: None,
            ticket: Some(self.ticket(&ticket, change_id, status)),
            seq: landed.then_some(7),
            landed_hash: landed.then(|| hash_to_proto(&fake_hash("landed"))),
            error: (status == pb::LandStatus::Failed).then(|| "boom".to_string()),
        }))
    }

    unimplemented_rpcs! {
        get_review(pb::GetReviewRequest) -> pb::GetReviewResponse;
        list_reviews(pb::ListReviewsRequest) -> pb::ListReviewsResponse;
        vote(pb::VoteRequest) -> pb::VoteResponse;
        comment(pb::CommentRequest) -> pb::CommentResponse;
        reply_thread(pb::ReplyThreadRequest) -> pb::ReplyThreadResponse;
        resolve_thread(pb::ResolveThreadRequest) -> pb::ResolveThreadResponse;
        evaluate_submit(pb::EvaluateSubmitRequest) -> pb::EvaluateSubmitResponse;
        abandon(pb::AbandonRequest) -> pb::AbandonResponse;
        watch_review(pb::WatchReviewRequest) -> Self::WatchReviewStream;
    }
}
