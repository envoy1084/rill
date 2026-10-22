//! Listing and creating repositories on the server.

use rill_proto::convert::id_to_proto;
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
            .list_repos(pb::ListReposRequest { limit, page_token })
            .await?
            .into_owned();
        let items = resp
            .repos
            .into_iter()
            .map(RepoInfo::try_from)
            .collect::<Result<_>>()?;
        Ok(Page::new(items, resp.next_page_token))
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
        let request = pb::CreateRepoRequest {
            operation_id: id_to_proto(&op),
            repo: name.to_string(),
            description,
            windows_safe,
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
