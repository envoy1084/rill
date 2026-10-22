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
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// How often a running request renews its claim, well inside
/// [`rill_store::repo::CLAIM_TTL_MS`].
const RENEW_EVERY: std::time::Duration =
    std::time::Duration::from_millis(rill_store::repo::CLAIM_TTL_MS as u64 / 3);

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
    RequestRedactionRequest,
    ApproveRedactionRequest,
    ClearQuarantineRequest,
    RemoveRelationRequest,
    SetPolicyRequest,
    CreateRepoRequest,
    CreateChangeVersionRequest,
    ApplyEditsRequest,
    RebaseRequest,
    MergeRequest,
    ResolveRequest,
    MoveRefRequest,
    CutReleaseRequest,
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
    RequestLandStackRequest,
    AppendTrunkRequest,
    RevokeTokenRequest,
    RevokeSessionRequest,
    RotateTokenKeysRequest,
    RotateDataKeysRequest,
    DeleteSecretRequest,
    QuarantineRequest,
    FreezeTrunkRequest,
    ReleaseQuarantineRequest,
    PutIdentityProviderRequest,
    DeleteIdentityProviderRequest,
    UpdateUserRequest,
    RegisterAgentRequest,
    UpdateAgentRequest,
    PutWorkloadIssuerRequest,
    DeleteWorkloadIssuerRequest,
    KillSwitchRequest,
    CreateScimCredentialRequest,
    RevokeScimCredentialRequest,
);

/// The idempotency key of the request being handled, for work that
/// records what it committed in the same transaction.
#[derive(Debug, Clone)]
pub struct Scope {
    pub principal: String,
    pub operation: Vec<u8>,
    pub digest: [u8; 32],
    /// What an earlier attempt of this request already committed.
    pub prior: Option<bytes::Bytes>,
}

tokio::task_local! {
    static SCOPE: Scope;
}

/// The scope of the request this task is handling, if it has one.
pub fn current() -> Option<Scope> {
    SCOPE.try_with(Clone::clone).ok()
}

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
    let prior = match stores.idem_claim(&principal, &operation, digest).await? {
        Claim::Fresh { prior } => prior,
        Claim::Replay(bytes) => {
            let response = Resp::decode(bytes)
                .map_err(|e| Error::Internal(format!("stored response: {e}")))?;
            return Ok(Response::new(response));
        }
        Claim::Running => {
            return Err(Error::Unavailable(
                "this operation is already running; retry later".into(),
            ))
        }
        Claim::Mismatch => {
            return Err(Error::InvalidArgument(
                "operation_id was already used for a different request".into(),
            ))
        }
    };
    let scope = Scope {
        principal: principal.clone(),
        operation: operation.clone(),
        digest,
        prior,
    };
    let renew = async {
        loop {
            tokio::time::sleep(RENEW_EVERY).await;
            match stores.idem_renew(&principal, &operation, digest).await {
                Ok(true) => {}
                Ok(false) => return,
                Err(e) => tracing::warn!(error = %e, "renewing an idempotency claim failed"),
            }
        }
    };
    let outcome = tokio::select! {
        outcome = SCOPE.scope(scope, handler(request)) => outcome,
        _ = renew => Err(Error::Unavailable(
            "this operation's claim was taken over; retry".into(),
        )),
    };
    match outcome {
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
