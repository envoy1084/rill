//! The repository-scoped client handle and its sub-clients.

use rill_proto::rill::v1 as pb;

use crate::client::Client;
use crate::error::Result;
use crate::intent::IntentClient;
use crate::lander::LanderClient;
use crate::review::ReviewClient;
use crate::stack::StackClient;
use crate::types::RepoInfo;
use crate::workspace::WorkspaceClient;

/// Operations on one repository. Every request carries the repository
/// name or id this client was built with.
#[derive(Clone, Debug)]
pub struct RepoClient {
    pub(super) client: Client,
    pub(super) repo: String,
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

    pub(crate) fn grants(&self) -> &crate::grants::ReadGrants {
        self.client.read_grants()
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
