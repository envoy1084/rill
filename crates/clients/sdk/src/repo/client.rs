//! The repository-scoped client handle and its sub-clients.

use rill_proto::rill::v1 as pb;

use crate::client::Client;
use crate::error::Result;
use crate::intent::IntentClient;
use crate::review::ReviewClient;
use crate::stack::StackClient;
use crate::trunk::TrunkClient;
use crate::types::RepoInfo;
use crate::workspace::WorkspaceClient;

/// Operations on one repository. Every request carries the repository
/// name or id this client was built with.
#[derive(Clone, Debug)]
pub struct RepoClient {
    pub(super) client: Client,
    pub(super) repo: String,
    /// The full resource name, `orgs/{org}/repos/{repo}`.
    pub(super) resource: String,
}

impl RepoClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        let resource = crate::names::repo(&repo);
        RepoClient {
            client,
            repo,
            resource,
        }
    }

    /// The repository's full resource name, `orgs/{org}/repos/{repo}`.
    pub fn resource(&self) -> &str {
        &self.resource
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
        WorkspaceClient::new(self.client.clone(), self.resource.clone())
    }

    /// Reviews in this repository.
    pub fn reviews(&self) -> ReviewClient {
        ReviewClient::new(self.client.clone(), self.resource.clone())
    }

    /// Intents in this repository.
    pub fn intents(&self) -> IntentClient {
        IntentClient::new(self.client.clone(), self.resource.clone())
    }

    /// Trunk status, verification, healing and reverts.
    pub fn trunk(&self) -> TrunkClient {
        TrunkClient::new(self.client.clone(), self.resource.clone())
    }

    /// Stack helpers composed from review and repo calls.
    pub fn stack(&self) -> StackClient {
        StackClient::new(self.clone())
    }

    pub(crate) fn svc(&self) -> crate::client::RepoSvc {
        self.client.repo_svc()
    }

    pub(crate) fn content(&self) -> crate::client::ContentSvc {
        self.client.content_svc()
    }

    pub(crate) fn changes(&self) -> crate::client::ChangeSvc {
        self.client.change_svc()
    }

    pub(crate) fn events(&self) -> crate::client::EventSvc {
        self.client.event_svc()
    }

    pub(crate) fn grants(&self) -> &crate::grants::ReadGrants {
        self.client.read_grants()
    }

    /// The repository record.
    pub async fn info(&self) -> Result<RepoInfo> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetRepoRequest {
                name: self.resource.clone(),
            };
            async move { svc.get_repo(request).await }
        })
        .await?
        .into_owned();
        rill_proto::convert::required("get_repo.repo", resp.repo)?.try_into()
    }
}
