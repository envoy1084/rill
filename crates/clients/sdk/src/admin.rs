//! Admin: the caller, capability tokens, policies, orgs, importing
//! history and health.

use rill_proto::convert::{required, timestamp_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::OperationId;

use crate::client::Client;
use crate::error::Result;
use crate::names;
use crate::types::{Health, IssuedToken, OrgInfo, Policy, TokenRequest, WhoAmI};

/// Instance-wide administrative operations; not scoped to a repository.
#[derive(Clone, Debug)]
pub struct AdminClient {
    pub(crate) client: Client,
}

impl AdminClient {
    pub(crate) fn new(client: Client) -> Self {
        AdminClient { client }
    }

    /// The caller's identity and capabilities.
    pub async fn whoami(&self) -> Result<WhoAmI> {
        crate::client::retry_idempotent(|| {
            let svc = self.client.identity_svc();
            async move { svc.get_caller(pb::GetCallerRequest {}).await }
        })
        .await?
        .into_owned()
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
            request_id: names::request_id(&op),
            capabilities: request.capabilities.iter().map(|c| c.to_proto()).collect(),
            expires_at: request.expires_at.map(|t| t.millis()),
            for_principal: request.for_principal.map(pb::Principal::from).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.identity_svc(), request.clone());
            async move { svc.issue_token(request).await }
        })
        .await?
        .into_owned();
        Ok(IssuedToken {
            token: resp.token,
            expires_at: timestamp_from_proto(resp.expires_at),
        })
    }

    /// `repo`'s `policy` (`submit` or `risk`).
    pub async fn policy(&self, repo: &str, policy: &str) -> Result<Policy> {
        let name = names::policy(repo, policy);
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.client.policy_svc();
            let request = pb::GetPolicyRequest { name: name.clone() };
            async move { svc.get_policy(request).await }
        })
        .await?
        .into_owned();
        required("get_policy.policy", resp.policy)?.try_into()
    }

    /// Replace `repo`'s `policy` text; with `etag`, only if it is unchanged
    /// since it was read.
    pub async fn set_policy(
        &self,
        repo: &str,
        policy: &str,
        text: &str,
        etag: Option<&str>,
    ) -> Result<Policy> {
        let request = pb::UpdatePolicyRequest {
            request_id: names::request_id(&OperationId::new()),
            policy: pb::Policy {
                name: names::policy(repo, policy),
                text: text.to_string(),
                ..Default::default()
            }
            .into(),
            etag: etag.unwrap_or_default().to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.policy_svc(), request.clone());
            async move { svc.update_policy(request).await }
        })
        .await?
        .into_owned();
        required("update_policy.policy", resp.policy)?.try_into()
    }

    /// Append an imported `version` to `repo`'s trunk if trunk still ends
    /// at `expected`; for importers of existing history.
    pub async fn append_trunk(
        &self,
        repo: &str,
        expected: Option<rill_types::ChangeVersionHash>,
        version: rill_types::ChangeVersionHash,
    ) -> Result<rill_types::TrunkSeq> {
        let request = pb::ImportTrunkPositionRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: names::repo(repo),
            expected: expected.map(|h| rill_proto::convert::hash_to_proto(&h)),
            version: rill_proto::convert::hash_to_proto(&version),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.admin_svc(), request.clone());
            async move { svc.import_trunk_position(request).await }
        })
        .await?
        .into_owned();
        Ok(rill_types::TrunkSeq(resp.position))
    }

    /// Create an org.
    pub async fn create_org(&self, name: &str, description: Option<&str>) -> Result<OrgInfo> {
        let request = pb::CreateOrgRequest {
            request_id: names::request_id(&OperationId::new()),
            org_id: name.to_string(),
            description: description.map(str::to_string),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.client.org_svc(), request.clone());
            async move { svc.create_org(request).await }
        })
        .await?
        .into_owned();
        Ok(required("create_org.org", resp.org)?.into())
    }

    /// One org.
    pub async fn org(&self, name: &str) -> Result<OrgInfo> {
        let name = names::org(name);
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.client.org_svc();
            let request = pb::GetOrgRequest { name: name.clone() };
            async move { svc.get_org(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_org.org", resp.org)?.into())
    }

    /// Every org.
    pub async fn list_orgs(&self) -> Result<Vec<OrgInfo>> {
        crate::paging::all(|page_token| async move {
            let resp = crate::client::retry_idempotent(|| {
                let svc = self.client.org_svc();
                let request = pb::ListOrgsRequest {
                    page_size: crate::paging::PAGE,
                    page_token: page_token.clone(),
                };
                async move { svc.list_orgs(request).await }
            })
            .await?
            .into_owned();
            let orgs = resp.orgs.into_iter().map(OrgInfo::from).collect();
            Ok((orgs, resp.next_page_token))
        })
        .await
    }

    /// Server health.
    pub async fn health(&self) -> Result<Health> {
        Ok(crate::client::retry_idempotent(|| {
            let svc = self.client.admin_svc();
            async move { svc.get_health(pb::GetHealthRequest {}).await }
        })
        .await?
        .into_owned()
        .into())
    }
}
