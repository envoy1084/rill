//! Repository-scoped operations: history, trees, blobs, diffs, writes,
//! refs, watches and derived data.

mod blobs;
mod compare;
mod derived;
mod history;
mod refs;
mod search;
mod tree;
mod watch;
mod write;

use rill_proto::convert::id_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::OperationId;

use crate::client::Client;
use crate::error::Result;
use crate::intent::IntentClient;
use crate::lander::LanderClient;
use crate::review::ReviewClient;
use crate::stack::StackClient;
use crate::types::{Page, RepoInfo};
use crate::workspace::WorkspaceClient;

pub use blobs::PUT_BLOB_CHUNK_SIZE;
pub use tree::DirListing;
pub(crate) use watch::convert_stream as watch_stream;
pub use watch::WatchStart;

/// Operations on one repository. Every request carries the repository
/// name or id this client was built with.
#[derive(Clone, Debug)]
pub struct RepoClient {
    client: Client,
    repo: String,
}

impl RepoClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        RepoClient { client, repo }
    }

    /// The repository name or id requests are addressed to.
    pub fn name(&self) -> &str {
        &self.repo
    }

    /// The underlying connection.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Workspaces in this repository.
    pub fn workspaces(&self) -> WorkspaceClient {
        WorkspaceClient::new(self.client.clone(), self.repo.clone())
    }

    /// Reviews in this repository.
    pub fn reviews(&self) -> ReviewClient {
        ReviewClient::new(self.client.clone(), self.repo.clone())
    }

    /// Intents in this repository.
    pub fn intents(&self) -> IntentClient {
        IntentClient::new(self.client.clone(), self.repo.clone())
    }

    /// Trunk status, verification and reverts.
    pub fn lander(&self) -> LanderClient {
        LanderClient::new(self.client.clone(), self.repo.clone())
    }

    /// Stack helpers composed from review and repo calls.
    pub fn stack(&self) -> StackClient {
        StackClient::new(self.clone())
    }

    pub(crate) fn svc(&self) -> crate::client::RepoSvc {
        self.client.repo_svc()
    }

    /// The repository record.
    pub async fn info(&self) -> Result<RepoInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::GetRepoRequest {
                repo: self.repo.clone(),
            };
            async move { svc.get_repo(request).await }
        })
        .await?
        .into_inner();
        rill_proto::convert::required("get_repo.repo", resp.repo)?.try_into()
    }
}

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
            .into_inner();
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
            let (mut svc, request) = (self.repo_svc(), request.clone());
            async move { svc.create_repo(request).await }
        })
        .await?
        .into_inner();
        rill_proto::convert::required("create_repo.repo", resp.repo)?.try_into()
    }
}
