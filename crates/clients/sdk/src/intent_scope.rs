//! Approving intents and asking whether a change inherits its intent's
//! approval.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Intent, IntentId, OperationId};

use crate::error::Result;
use crate::intent::IntentClient;
use crate::names;
use crate::types::IntentScopeExplanation;

impl IntentClient {
    /// Approve revision `revision` of the intent's scope, for the scope
    /// directories the caller owns.
    pub async fn approve(&self, id: IntentId, revision: u32) -> Result<Intent> {
        let request = pb::ApproveIntentRequest {
            request_id: names::request_id(&OperationId::new()),
            name: self.name(&id),
            revision,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.approve_intent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("approve_intent.intent", resp.intent)?.try_into()?)
    }

    /// Withdraw the caller's approval.
    pub async fn revoke_approval(&self, id: IntentId) -> Result<Intent> {
        let request = pb::RevokeIntentApprovalRequest {
            request_id: names::request_id(&OperationId::new()),
            name: self.name(&id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.revoke_intent_approval(request).await }
        })
        .await?
        .into_owned();
        Ok(required("revoke_intent_approval.intent", resp.intent)?.try_into()?)
    }

    /// Whether `change`'s intent is approved and the change inside its
    /// scope.
    pub async fn explain_scope(&self, change: ChangeId) -> Result<IntentScopeExplanation> {
        let request = pb::ExplainIntentScopeRequest {
            change: names::change(self.repo_name(), &change),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.explain_intent_scope(request).await }
        })
        .await?
        .into_owned();
        Ok(IntentScopeExplanation {
            intent: resp
                .intent
                .rsplit('/')
                .next()
                .and_then(|id| id.parse::<IntentId>().ok()),
            approved: resp.approved,
            in_scope: resp.in_scope,
            reasons: resp.reasons,
            missing_approvals: resp.missing_approvals,
        })
    }
}
