//! Retry safety for every mutating RPC.
//!
//! A request with a non-empty `operation_id` runs at most once per caller:
//! a retry after a lost reply gets the first response back. See
//! [`rill_store::repo::Claim`] for the protocol.

use std::future::Future;

use buffa::Message;
use rill_proto::rill::v1 as pb;
use rill_store::repo::Claim;

use crate::auth::caller;
use crate::services::Call;
use connectrpc::Response;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// How often a running request renews its claim, well inside
/// [`rill_store::repo::CLAIM_TTL_MS`].
const RENEW_EVERY: std::time::Duration =
    std::time::Duration::from_millis(rill_store::repo::CLAIM_TTL_MS as u64 / 3);

/// A request message carrying an idempotency key.
pub trait Idempotent: Message {
    /// The key as 16 bytes; empty when the request has none.
    fn key(&self) -> Result<Vec<u8>>;
}

/// Requests keyed by a `request_id` UUID.
macro_rules! idempotent {
    ($($t:ident),* $(,)?) => {
        $(impl Idempotent for pb::$t {
            fn key(&self) -> Result<Vec<u8>> {
                if self.request_id.is_empty() {
                    return Ok(Vec::new());
                }
                Ok(super::names::request_id(&self.request_id)?.as_bytes().to_vec())
            }
        })*
    };
}

idempotent!(
    ApplyEditsRequest,
    CreateRepoRequest,
    UpdateRefRequest,
    DeleteRefRequest,
    CreateReleaseRequest,
    CreateVersionRequest,
    RebaseVersionRequest,
    MergeVersionsRequest,
    ResolveConflictsRequest,
    CreateWorkspaceRequest,
    UpdateWorkspaceRequest,
    DeleteWorkspaceRequest,
    SnapshotWorkspaceRequest,
    StartChangeRequest,
    SyncWorkspaceRequest,
    HandoffWorkspaceRequest,
    UndoOperationRequest,
    SubmitChangeRequest,
    VoteRequest,
    CreateThreadRequest,
    ReplyThreadRequest,
    ResolveThreadRequest,
    UpdateStackRequest,
    AbandonChangeRequest,
    CancelLandRequestRequest,
    RequestLandRequest,
    RequestLandStackRequest,
    ReportVerificationRequest,
    RevertLandingRequest,
    BulkRevertRequest,
    BisectRequest,
    FreezeTrunkRequest,
    UnfreezeTrunkRequest,
    CreateIntentRequest,
    UpdateIntentRequest,
    LinkChangeRequest,
    CreateQuarantineRequest,
    DeleteQuarantineRequest,
    IssueTokenRequest,
    RevokeTokenRequest,
    RevokeSessionRequest,
    RotateTokenKeysRequest,
    UpdateIdentityProviderRequest,
    DeleteIdentityProviderRequest,
    UpdateUserRequest,
    CreateAgentRequest,
    UpdateAgentRequest,
    UpdateWorkloadIssuerRequest,
    DeleteWorkloadIssuerRequest,
    KillSwitchRequest,
    CreateScimCredentialRequest,
    DeleteScimCredentialRequest,
    CreateOrgRequest,
    CreateRelationRequest,
    DeleteRelationRequest,
    UpdatePolicyRequest,
    ImportTrunkPositionRequest,
    CreateRedactionRequest,
    ApproveRedactionRequest,
    ReleaseVersionRequest,
    RotateDataKeysRequest,
    DeleteSecretRequest,
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
    request: Call<Req>,
    handler: F,
) -> Result<Response<Resp>>
where
    Req: Idempotent,
    Resp: Message,
    F: FnOnce(Call<Req>) -> Fut,
    Fut: Future<Output = Result<Response<Resp>>>,
{
    let operation = request.get_ref().key()?;
    if operation.is_empty() {
        return handler(request).await;
    }
    let principal = caller(&request)?.principal.id.to_string();
    let digest = *blake3::hash(&request.get_ref().encode_to_vec()).as_bytes();
    let stores = &state.stores;
    let prior = match stores.idem_claim(&principal, &operation, digest).await? {
        Claim::Fresh { prior } => prior,
        Claim::Replay(bytes) => {
            let response = Resp::decode_from_slice(&bytes)
                .map_err(|e| Error::Internal(format!("stored response: {e}")))?;
            return Ok(Response::new(response));
        }
        Claim::Running => {
            return Err(Error::Unavailable(
                "this operation is already running; retry later".into(),
            ))
        }
        Claim::Mismatch => return Err(Error::RequestIdReused),
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
                    &response.body.encode_to_vec(),
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
