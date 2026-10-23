//! Review and landing services whose land requests follow a scripted
//! status sequence.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use bytes::Bytes;
use connectrpc::{
    ConnectError, InboundStream, RequestContext, Response, ServiceRequest, ServiceStream,
};
use futures::stream::BoxStream;
use rill_proto::convert::{hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::{LandingService, ReviewService};
use rill_types::{ChangeId, Principal};

use super::fixtures::fake_hash;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

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

    fn ticket(&self, id: &str, change_id: Bytes, status: pb::LandStatus) -> pb::LandRequest {
        pb::LandRequest {
            name: format!("orgs/default/repos/main/landRequests/{id}"),
            ticket: id.to_string(),
            change_id,
            status: status.into(),
            requested_at: 1,
            ..Default::default()
        }
    }

    fn review_state(change_id: Bytes, stack: Vec<Bytes>) -> pb::ReviewState {
        pb::ReviewState {
            name: String::new(),
            change_id,
            versions: vec![hash_to_proto(&fake_hash("v1"))],
            status: pb::ReviewStatus::Open.into(),
            intent: None,
            owner: (pb::Principal::from(Principal::human("alice"))).into(),
            votes: vec![],
            threads: vec![],
            risk: pb::RiskClass::Normal.into(),
            risk_reasons: vec![],
            stack,
            landed_seq: None,
            created_at: 1,
            updated_at: 1,
            extra: BTreeMap::new(),
        }
    }
}

/// The id of the change a `…/changes/{change}` name names.
fn change_of(name: &str) -> Result<ChangeId, ConnectError> {
    name.rsplit('/')
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| ConnectError::invalid_argument(format!("bad change name {name:?}")))
}

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl ReviewService for MockReview {
    async fn submit_change(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::SubmitChangeRequest>,
    ) -> RpcResult<pb::SubmitChangeResponse> {
        let req = Request::new(ctx, req).into_inner();
        let id = change_of(&req.change)?;
        self.submitted.lock().unwrap().push(id);
        Ok(Response::new(pb::SubmitChangeResponse {
            review: Self::review_state(id_to_proto(&id), vec![]).into(),
        }))
    }

    async fn update_stack(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::UpdateStackRequest>,
    ) -> RpcResult<pb::UpdateStackResponse> {
        let req = Request::new(ctx, req).into_inner();
        let ids = req
            .changes
            .iter()
            .map(|c| change_of(c))
            .collect::<Result<Vec<_>, _>>()?;
        self.stacks.lock().unwrap().push(ids.clone());
        let stack: Vec<Bytes> = ids.iter().map(id_to_proto).collect();
        Ok(Response::new(pb::UpdateStackResponse {
            reviews: stack
                .iter()
                .map(|id| Self::review_state(id.clone(), stack.clone()))
                .collect(),
        }))
    }

    unimplemented_rpcs! {
        get_review(pb::GetReviewRequest) -> pb::GetReviewResponse;
        batch_get_reviews(pb::BatchGetReviewsRequest) -> pb::BatchGetReviewsResponse;
        list_reviews(pb::ListReviewsRequest) -> pb::ListReviewsResponse;
        get_owners(pb::GetOwnersRequest) -> pb::GetOwnersResponse;
        explain_ownership(pb::ExplainOwnershipRequest) -> pb::ExplainOwnershipResponse;
        vote(pb::VoteRequest) -> pb::VoteResponse;
        create_thread(pb::CreateThreadRequest) -> pb::CreateThreadResponse;
        reply_thread(pb::ReplyThreadRequest) -> pb::ReplyThreadResponse;
        resolve_thread(pb::ResolveThreadRequest) -> pb::ResolveThreadResponse;
        evaluate_landing(pb::EvaluateLandingRequest) -> pb::EvaluateLandingResponse;
        abandon_change(pb::AbandonChangeRequest) -> pb::AbandonChangeResponse;
        watch_review(pb::WatchReviewRequest) -> ServiceStream<pb::WatchReviewResponse>;
        get_interdiff(pb::GetInterdiffRequest) -> pb::GetInterdiffResponse;
        apply_suggestion(pb::ApplySuggestionRequest) -> pb::ApplySuggestionResponse;
        reopen_change(pb::ReopenChangeRequest) -> pb::ReopenChangeResponse;
    }
}

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl LandingService for MockReview {
    async fn request_land(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::RequestLandRequest>,
    ) -> RpcResult<pb::RequestLandResponse> {
        let req = Request::new(ctx, req).into_inner();
        let id = change_of(&req.change)?;
        self.land_requests.lock().unwrap().push(id);
        Ok(Response::new(pb::RequestLandResponse {
            land_request: (self.ticket(
                &format!("ticket-{id}"),
                id_to_proto(&id),
                pb::LandStatus::Queued,
            ))
            .into(),
        }))
    }

    async fn request_land_stack(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::RequestLandStackRequest>,
    ) -> RpcResult<pb::RequestLandStackResponse> {
        let req = Request::new(ctx, req).into_inner();
        let id = change_of(&req.change)?;
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
            land_requests: stack
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

    async fn cancel_land_request(
        &self,
        _ctx: RequestContext,
        _req: ServiceRequest<'_, pb::CancelLandRequestRequest>,
    ) -> RpcResult<pb::CancelLandRequestResponse> {
        Err(ConnectError::new(
            connectrpc::ErrorCode::Unimplemented,
            "not mocked",
        ))
    }

    async fn get_land_request(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetLandRequestRequest>,
    ) -> RpcResult<pb::GetLandRequestResponse> {
        let name = Request::new(ctx, req).into_inner().name;
        let ticket = name.rsplit('/').next().unwrap_or_default().to_string();
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
        Ok(Response::new(pb::GetLandRequestResponse {
            land_request: pb::LandRequest {
                trunk_position: landed.then_some(7),
                landed_hash: landed.then(|| hash_to_proto(&fake_hash("landed"))),
                error: (status == pb::LandStatus::Failed).then(|| "boom".to_string()),
                ..self.ticket(&ticket, change_id, status)
            }
            .into(),
        }))
    }

    unimplemented_rpcs! {
        list_land_requests(pb::ListLandRequestsRequest) -> pb::ListLandRequestsResponse;
    }
}
