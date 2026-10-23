//! Code owners through the review service.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, RepoPath};

use crate::error::Result;
use crate::review::ReviewClient;
use crate::types::{OwnerSet, Ownership};

impl ReviewClient {
    /// The owner sets covering `path` on trunk.
    pub async fn owners_of(&self, path: &RepoPath) -> Result<Vec<OwnerSet>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetOwnersRequest {
                repo: self.repo_name().to_string(),
                path: path.to_string(),
            };
            async move { svc.get_owners(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.sets.into_iter().map(OwnerSet::from).collect())
    }

    /// Each owner set `change_id` touches, with its approvals.
    pub async fn ownership(&self, change_id: ChangeId) -> Result<Ownership> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ExplainOwnershipRequest {
                change: self.change_name(&change_id),
            };
            async move { svc.explain_ownership(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.into())
    }
}
