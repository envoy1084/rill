//! Retry safety for every mutating RPC.
//!
//! A request with a non-empty `operation_id` runs at most once per caller:
//! a retry after a lost reply gets the first response back. See
//! [`rill_store::repo::Claim`] for the protocol.

use std::future::Future;

use prost::Message;
use rill_proto::rill::v1 as pb;
use rill_store::repo::Claim;
use tonic::{Request, Response};

use crate::auth::caller;
use crate::error::{Error, Result};
use crate::state::AppState;

/// A request message carrying an idempotency key.
pub trait Idempotent: Message {
    fn operation_id(&self) -> &[u8];
}

macro_rules! idempotent {
    ($($t:ident),* $(,)?) => {
        $(impl Idempotent for pb::$t {
            fn operation_id(&self) -> &[u8] {
                &self.operation_id
            }
        })*
    };
}

idempotent!(
    ReportVerificationRequest,
    RevertSeqRequest,
    BisectRequest,
    CreateIntentRequest,
    UpdateIntentRequest,
    LinkChangeRequest,
    CreateOrgRequest,
    IssueTokenRequest,
    AddRelationRequest,
    RemoveRelationRequest,
    SetPolicyRequest,
    CreateRepoRequest,
    CreateChangeVersionRequest,
    ApplyEditsRequest,
    RebaseRequest,
    MergeRequest,
    ResolveRequest,
    MoveRefRequest,
    CreateWorkspaceRequest,
    SetSparseProfileRequest,
    SnapshotRequest,
    NewChangeRequest,
    UpdateBaseRequest,
    HandoffRequest,
    UndoRequest,
    DeleteWorkspaceRequest,
    SubmitForReviewRequest,
    VoteRequest,
    CommentRequest,
    ReplyThreadRequest,
    ResolveThreadRequest,
    SetStackRequest,
    AbandonRequest,
    RequestLandRequest,
);

/// Run `handler` for `request` at most once per caller and operation id.
/// Requests without an id run every time.
pub async fn once<Req, Resp, F, Fut>(
    state: &AppState,
    request: Request<Req>,
    handler: F,
) -> Result<Response<Resp>>
where
    Req: Idempotent,
    Resp: Message + Default,
    F: FnOnce(Request<Req>) -> Fut,
    Fut: Future<Output = Result<Response<Resp>>>,
{
    let operation = request.get_ref().operation_id().to_vec();
    if operation.is_empty() {
        return handler(request).await;
    }
    let principal = caller(&request)?.principal.id.to_string();
    let digest = *blake3::hash(&request.get_ref().encode_to_vec()).as_bytes();
    let stores = &state.stores;
    match stores.idem_claim(&principal, &operation, digest).await? {
        Claim::Fresh => {}
        Claim::Replay(bytes) => {
            let response = Resp::decode(bytes)
                .map_err(|e| Error::Internal(format!("stored response: {e}")))?;
            return Ok(Response::new(response));
        }
        Claim::Running => {
            return Err(Error::FailedPrecondition(
                "this operation is already running; retry later".into(),
            ))
        }
        Claim::Mismatch => {
            return Err(Error::InvalidArgument(
                "operation_id was already used for a different request".into(),
            ))
        }
    }
    match handler(request).await {
        Ok(response) => {
            stores
                .idem_finish(
                    &principal,
                    &operation,
                    digest,
                    &response.get_ref().encode_to_vec(),
                )
                .await?;
            Ok(response)
        }
        Err(e) => {
            if let Err(release) = stores.idem_release(&principal, &operation).await {
                tracing::warn!(error = %release, "releasing an idempotency claim failed");
            }
            Err(e)
        }
    }
}
