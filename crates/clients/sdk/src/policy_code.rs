//! Policy as code: the policy in force, why a landing was allowed,
//! replaying history under a candidate, and org guardrails.

use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, TrunkSeq};

use crate::admin::AdminClient;
use crate::error::Result;
use crate::names;

impl AdminClient {
    /// The submit rules in force in `repo`, with where they came from.
    pub async fn effective_policy(&self, repo: &str) -> Result<pb::GetEffectivePolicyResponse> {
        let request = pb::GetEffectivePolicyRequest {
            parent: names::repo(repo),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.get_effective_policy(request).await }
        })
        .await?
        .into_owned())
    }

    /// Why the landing at `seq` in `repo` was allowed.
    pub async fn explain_landing(
        &self,
        repo: &str,
        seq: TrunkSeq,
    ) -> Result<pb::ExplainLandingResponse> {
        let request = pb::ExplainLandingRequest {
            landing: format!("{}/trunk/positions/{}", names::repo(repo), seq.0),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.explain_landing(request).await }
        })
        .await?
        .into_owned())
    }

    /// Replay `repo`'s landings `since..=until` (0: the first / the last)
    /// under the candidate policy `text`.
    pub async fn simulate_policy(
        &self,
        repo: &str,
        text: &str,
        since: u64,
        until: u64,
    ) -> Result<pb::SimulatePolicyResponse> {
        let request = pb::SimulatePolicyRequest {
            parent: names::repo(repo),
            text: text.to_string(),
            since,
            until,
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.simulate_policy(request).await }
        })
        .await?
        .into_owned())
    }

    /// `org`'s guardrails.
    pub async fn guardrails(&self, org: &str) -> Result<String> {
        let request = pb::GetGuardrailsRequest {
            org: names::org(org),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.get_guardrails(request).await }
        })
        .await?
        .into_owned()
        .text)
    }

    /// Replace `org`'s guardrails; empty removes them.
    pub async fn set_guardrails(&self, org: &str, text: &str) -> Result<String> {
        let request = pb::UpdateGuardrailsRequest {
            request_id: names::request_id(&OperationId::new()),
            org: names::org(org),
            text: text.to_string(),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.update_guardrails(request).await }
        })
        .await?
        .into_owned()
        .text)
    }
}
