//! Admin: identity, capability tokens, policies and health.

use rill_proto::convert::{id_to_proto, timestamp_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, Timestamp};

use crate::client::Client;
use crate::error::Result;
use crate::types::{Health, IssuedToken, OrgInfo, Policy, TokenRequest, WhoAmI};

/// Instance-wide administrative operations; not scoped to a repository.
#[derive(Clone, Debug)]
pub struct AdminClient {
    client: Client,
}

impl AdminClient {
    pub(crate) fn new(client: Client) -> Self {
        AdminClient { client }
    }

    fn svc(&self) -> crate::client::AdminSvc {
        self.client.admin_svc()
    }

    /// The caller's identity and capabilities.
    pub async fn whoami(&self) -> Result<WhoAmI> {
        crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::WhoAmIRequest {};
            async move { svc.who_am_i(request).await }
        })
        .await?
        .into_inner()
        .try_into()
    }

    /// Mint a token attenuated to a subset of the caller's capabilities.
    pub async fn issue_token(&self, request: TokenRequest) -> Result<IssuedToken> {
        self.issue_token_with_op(OperationId::new(), request).await
    }

    /// [`AdminClient::issue_token`] with a caller-chosen idempotency key.
    pub async fn issue_token_with_op(
        &self,
        op: OperationId,
        request: TokenRequest,
    ) -> Result<IssuedToken> {
        let request = pb::IssueTokenRequest {
            operation_id: id_to_proto(&op),
            capabilities: request.capabilities.iter().map(|c| c.to_proto()).collect(),
            expires_at: request.expires_at.map(|t| t.millis()),
            for_principal: request.for_principal.map(pb::Principal::from),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.issue_token(request).await }
        })
        .await?
        .into_inner();
        Ok(IssuedToken {
            token: resp.token,
            expires_at: timestamp_from_proto(resp.expires_at),
        })
    }

    /// A named policy.
    pub async fn policy(&self, name: &str) -> Result<Policy> {
        crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::GetPolicyRequest {
                name: name.to_string(),
            };
            async move { svc.get_policy(request).await }
        })
        .await?
        .into_inner()
        .try_into()
    }

    /// Replace a named policy's text. Returns the update time.
    pub async fn set_policy(&self, name: &str, text: &str) -> Result<Timestamp> {
        self.set_policy_with_op(OperationId::new(), name, text)
            .await
    }

    /// [`AdminClient::set_policy`] with a caller-chosen idempotency key.
    pub async fn set_policy_with_op(
        &self,
        op: OperationId,
        name: &str,
        text: &str,
    ) -> Result<Timestamp> {
        let request = pb::SetPolicyRequest {
            operation_id: id_to_proto(&op),
            name: name.to_string(),
            text: text.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.set_policy(request).await }
        })
        .await?
        .into_inner();
        Ok(timestamp_from_proto(resp.updated_at))
    }

    /// Append an imported `version` to `repo`'s trunk if trunk still ends
    /// at `expected`; for importers of existing history.
    pub async fn append_trunk(
        &self,
        repo: &str,
        expected: Option<rill_types::ChangeVersionHash>,
        version: rill_types::ChangeVersionHash,
    ) -> Result<rill_types::TrunkSeq> {
        let request = pb::AppendTrunkRequest {
            operation_id: id_to_proto(&OperationId::new()),
            repo: repo.to_string(),
            expected: expected.map(|h| rill_proto::convert::hash_to_proto(&h)),
            version: rill_proto::convert::hash_to_proto(&version),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.append_trunk(request).await }
        })
        .await?
        .into_inner();
        Ok(rill_types::TrunkSeq(resp.seq))
    }

    /// Create an org.
    pub async fn create_org(&self, name: &str, description: Option<&str>) -> Result<OrgInfo> {
        let request = pb::CreateOrgRequest {
            operation_id: id_to_proto(&OperationId::new()),
            name: name.to_string(),
            description: description.map(str::to_string),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.create_org(request).await }
        })
        .await?
        .into_inner();
        Ok(rill_proto::convert::required("create_org.org", resp.org)?.into())
    }

    /// Every org.
    pub async fn list_orgs(&self) -> Result<Vec<OrgInfo>> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::ListOrgsRequest {};
            async move { svc.list_orgs(request).await }
        })
        .await?
        .into_inner();
        Ok(resp.orgs.into_iter().map(OrgInfo::from).collect())
    }

    /// Server health.
    pub async fn health(&self) -> Result<Health> {
        Ok(crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::HealthRequest {};
            async move { svc.health(request).await }
        })
        .await?
        .into_inner()
        .into())
    }
}
