//! Access: relationship tuples and questions about them.

use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, Principal};

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::names;
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
        let request = pb::CreateRelationRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: names::repo(&relation.repo),
            relation: relation.to_proto().into(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_relation(request).await }
        })
        .await?
        .into_owned();
        self.client
            .relations_seen()
            .observe(&resp.repo_uid, resp.relation_version);
        Ok(())
    }

    /// Remove a tuple.
    pub async fn revoke(&self, relation: &Relation) -> Result<()> {
        let request = pb::DeleteRelationRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: names::repo(&relation.repo),
            relation: relation.to_proto().into(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_relation(request).await }
        })
        .await?
        .into_owned();
        self.client
            .relations_seen()
            .observe(&resp.repo_uid, resp.relation_version);
        Ok(())
    }

    /// A repository's tuples (owners only).
    pub async fn list(&self, repo: &str, filter: RelationFilter) -> Result<Vec<Relation>> {
        let filter = filter.to_filter();
        crate::paging::all(|page_token| {
            let filter = filter.clone();
            async move {
                let request = pb::ListRelationsRequest {
                    parent: names::repo(repo),
                    page_size: crate::paging::PAGE,
                    page_token,
                    filter,
                };
                let resp = retry_idempotent(|| {
                    let (svc, request) = (self.svc(), request.clone());
                    async move { svc.list_relations(request).await }
                })
                .await?
                .into_owned();
                let relations = resp
                    .relations
                    .into_iter()
                    .map(|r| Relation::from_proto(repo, r))
                    .collect();
                Ok((relations, resp.next_page_token))
            }
        })
        .await
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
        let request = pb::CheckAccessRequest {
            parent: names::repo(repo),
            path: path.to_string(),
            relation: relation.to_string(),
            subject: subject.map(String::from),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.check_access(request).await }
        })
        .await?
        .into_owned();
        Ok(AccessCheck {
            allowed: resp.allowed,
            because: resp
                .because
                .into_iter()
                .map(|r| Relation::from_proto(repo, r))
                .collect(),
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
        let request = pb::ExpandAccessRequest {
            parent: names::repo(repo),
            path: path.to_string(),
            relation: relation.to_string(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.expand_access(request).await }
        })
        .await?
        .into_owned();
        Ok(resp
            .tuples
            .into_iter()
            .map(|t| ExpandedRelation {
                relation: Relation::from_proto(repo, t.tuple.into_option().unwrap_or_default()),
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
            parent: names::repo(repo),
            relation: relation.to_string(),
            subject: subject.map(String::from),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.lookup_paths(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.paths)
    }
}
