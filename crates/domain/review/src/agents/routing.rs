//! Trunk's `.rill/review.toml`.

use rill_policy::reviewers::{ReviewConfig, REVIEW_CONFIG};
use rill_types::RepoPathBuf;

use crate::error::Result;
use crate::service::ReviewService;

impl ReviewService {
    /// The reviewer routing on trunk's head; empty when there is none.
    /// A file that does not parse routes nowhere (commits are linted, so
    /// only an old trunk can hold one).
    pub async fn trunk_review_config(&self) -> Result<ReviewConfig> {
        let path = RepoPathBuf::new(REVIEW_CONFIG)?;
        let Some(bytes) = self.trunk_file(&path).await? else {
            return Ok(ReviewConfig::default());
        };
        match ReviewConfig::parse(&String::from_utf8_lossy(&bytes)) {
            Ok(config) => Ok(config),
            Err(e) => {
                tracing::warn!(error = %e, "ignoring trunk's review config");
                Ok(ReviewConfig::default())
            }
        }
    }
}

impl ReviewService {
    /// What the directory knows about `principal`.
    pub async fn person(&self, principal: &rill_types::Principal) -> Result<rill_policy::Person> {
        Ok(self.directory.person(principal).await?)
    }
}
