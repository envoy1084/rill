//! Access: relationship tuples and questions about them.

use rill_proto::convert::id_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, Principal};

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::types::{AccessCheck, ExpandedRelation, Relation, RelationFilter};

/// Who may do what in repositories. Grants and revokes made through a
/// client are honoured by every server on that client's next request.
#[derive(Clone, Debug)]
pub struct AccessClient {
    client: Client,
}

impl AccessClient {
    pub(crate) fn new(client: Client) -> Self {
        AccessClient { client }
    }

    fn svc(&self) -> crate::client::AccessSvc {
        self.client.access_svc()
    }

    /// Add a tuple.
    pub async fn grant(&self, relation: &Relation) -> Result<()> {
        let request = pb::AddRelationRequest {
            operation_id: id_to_proto(&OperationId::new()),
            relation: Some(relation.to_proto()),
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.add_relation(request).await }
        })
        .await?
        .into_inner();
        self.client
            .relations_seen()
            .observe(&resp.repo_id, resp.relation_version);
        Ok(())
    }

    /// Remove a tuple.
    pub async fn revoke(&self, relation: &Relation) -> Result<()> {
        let request = pb::RemoveRelationRequest {
            operation_id: id_to_proto(&OperationId::new()),
            relation: Some(relation.to_proto()),
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.remove_relation(request).await }
        })
        .await?
        .into_inner();
        self.client
            .relations_seen()
            .observe(&resp.repo_id, resp.relation_version);
        Ok(())
    }

    /// A repository's tuples (owners only).
    pub async fn list(&self, repo: &str, filter: RelationFilter) -> Result<Vec<Relation>> {
        let request = pb::ListRelationsRequest {
            repo: repo.to_string(),
            path: filter.path,
            subject: filter.subject,
            relation: filter.relation,
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.list_relations(request).await }
        })
        .await?
        .into_inner();
        Ok(resp.relations.into_iter().map(Relation::from).collect())
    }

    /// Whether `subject` (the caller when `None`) holds `relation` on
    /// `path`.
    pub async fn check(
        &self,
        repo: &str,
        path: &str,
        relation: &str,
        subject: Option<&str>,
    ) -> Result<AccessCheck> {
        let request = pb::CheckRequest {
            repo: repo.to_string(),
            path: path.to_string(),
            relation: relation.to_string(),
            subject: subject.map(String::from),
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.check(request).await }
        })
        .await?
        .into_inner();
        Ok(AccessCheck {
            allowed: resp.allowed,
            because: resp.because.into_iter().map(Relation::from).collect(),
            org_admin: resp.org_admin,
        })
    }

    /// Every tuple granting `relation` on `path` (owners only).
    pub async fn expand(
        &self,
        repo: &str,
        path: &str,
        relation: &str,
    ) -> Result<Vec<ExpandedRelation>> {
        let request = pb::ExpandRequest {
            repo: repo.to_string(),
            path: path.to_string(),
            relation: relation.to_string(),
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.expand(request).await }
        })
        .await?
        .into_inner();
        Ok(resp
            .tuples
            .into_iter()
            .map(|t| ExpandedRelation {
                relation: t
                    .tuple
                    .map(Relation::from)
                    .unwrap_or_else(|| Relation::new("", "", "", "")),
                members: t
                    .members
                    .into_iter()
                    .filter_map(|p| Principal::try_from(p).ok())
                    .collect(),
            })
            .collect())
    }

    /// The directories where `subject` (the caller when `None`) holds
    /// `relation`; `""` is the whole repository.
    pub async fn lookup_paths(
        &self,
        repo: &str,
        relation: &str,
        subject: Option<&str>,
    ) -> Result<Vec<String>> {
        let request = pb::LookupPathsRequest {
            repo: repo.to_string(),
            relation: relation.to_string(),
            subject: subject.map(String::from),
        };
        let resp = retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.lookup_paths(request).await }
        })
        .await?
        .into_inner();
        Ok(resp.paths)
    }
}
