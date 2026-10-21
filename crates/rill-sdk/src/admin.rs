//! Admin: identity, capability tokens, relationships, policies and health.

use rill_proto::convert::{id_to_proto, timestamp_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, Timestamp};

use crate::client::Client;
use crate::error::Result;
use crate::types::{Health, IssuedToken, OrgInfo, Policy, Relation, TokenRequest, WhoAmI};

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
        self.svc()
            .who_am_i(pb::WhoAmIRequest {})
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

    /// Add a ReBAC tuple.
    pub async fn add_relation(&self, relation: &Relation) -> Result<()> {
        self.add_relation_with_op(OperationId::new(), relation)
            .await
    }

    /// [`AdminClient::add_relation`] with a caller-chosen idempotency key.
    pub async fn add_relation_with_op(&self, op: OperationId, relation: &Relation) -> Result<()> {
        let request = pb::AddRelationRequest {
            operation_id: id_to_proto(&op),
            relation: Some(relation.to_proto()),
        };
        crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.add_relation(request).await }
        })
        .await?;
        Ok(())
    }

    /// Remove a ReBAC tuple.
    pub async fn remove_relation(&self, relation: &Relation) -> Result<()> {
        self.remove_relation_with_op(OperationId::new(), relation)
            .await
    }

    /// [`AdminClient::remove_relation`] with a caller-chosen idempotency key.
    pub async fn remove_relation_with_op(
        &self,
        op: OperationId,
        relation: &Relation,
    ) -> Result<()> {
        let request = pb::RemoveRelationRequest {
            operation_id: id_to_proto(&op),
            relation: Some(relation.to_proto()),
        };
        crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.remove_relation(request).await }
        })
        .await?;
        Ok(())
    }

    /// Tuples on an object, optionally restricted to one relation.
    pub async fn list_relations(
        &self,
        object: &str,
        relation: Option<&str>,
    ) -> Result<Vec<Relation>> {
        let resp = self
            .svc()
            .list_relations(pb::ListRelationsRequest {
                object: object.to_string(),
                relation: relation.map(str::to_string),
            })
            .await?
            .into_inner();
        Ok(resp.relations.into_iter().map(Relation::from).collect())
    }

    /// A named policy.
    pub async fn policy(&self, name: &str) -> Result<Policy> {
        self.svc()
            .get_policy(pb::GetPolicyRequest {
                name: name.to_string(),
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
        let resp = self
            .svc()
            .list_orgs(pb::ListOrgsRequest {})
            .await?
            .into_inner();
        Ok(resp.orgs.into_iter().map(OrgInfo::from).collect())
    }

    /// Server health.
    pub async fn health(&self) -> Result<Health> {
        Ok(self
            .svc()
            .health(pb::HealthRequest {})
            .await?
            .into_inner()
            .into())
    }
}
