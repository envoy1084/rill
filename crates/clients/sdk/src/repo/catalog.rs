//! Listing and creating repositories on the server.

use rill_proto::rill::v1 as pb;
use rill_types::OperationId;

use crate::client::Client;
use crate::error::Result;
use crate::types::{Page, RepoInfo};

impl Client {
    /// List repositories on the server.
    pub async fn list_repos(
        &self,
        limit: u32,
        page_token: Option<String>,
    ) -> Result<Page<RepoInfo>> {
        let resp = self
            .repo_svc()
            .list_repos(pb::ListReposRequest {
                parent: format!("orgs/{}", crate::names::DEFAULT_ORG),
                page_size: limit as i32,
                page_token: page_token.unwrap_or_default(),
            })
            .await?
            .into_owned();
        let items = resp
            .repos
            .into_iter()
            .map(RepoInfo::try_from)
            .collect::<Result<_>>()?;
        let next = (!resp.next_page_token.is_empty()).then_some(resp.next_page_token);
        Ok(Page::new(items, next))
    }

    /// Create a repository.
    pub async fn create_repo(&self, name: &str, description: Option<String>) -> Result<RepoInfo> {
        self.create_repo_with_op(OperationId::new(), name, description, false)
            .await
    }

    /// [`Client::create_repo`] with a caller-chosen idempotency key.
    pub async fn create_repo_with_op(
        &self,
        op: OperationId,
        name: &str,
        description: Option<String>,
        windows_safe: bool,
    ) -> Result<RepoInfo> {
        let full = crate::names::repo(name);
        let (org, repo_id) =
            rill_proto::names::repo_of(&full).map_err(|e| crate::Error::config(e.to_string()))?;
        let request = pb::CreateRepoRequest {
            request_id: crate::names::request_id(&op),
            parent: format!("orgs/{org}"),
            repo_id: repo_id.to_string(),
            repo: pb::Repo {
                description,
                windows_safe,
                ..Default::default()
            }
            .into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.repo_svc(), request.clone());
            async move { svc.create_repo(request).await }
        })
        .await?
        .into_owned();
        rill_proto::convert::required("create_repo.repo", resp.repo)?.try_into()
    }
}
