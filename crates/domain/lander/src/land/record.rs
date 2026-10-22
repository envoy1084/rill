//! Landing records, written by the land transaction.

use rill_store::RepoStore;
use rill_types::ChangeVersionHash;

pub use rill_types::Landing;

use crate::error::Result;

/// How many recent landings listings return.
pub const RECENT_LANDINGS: usize = 1000;

/// The landing record of `version`, if it is on trunk.
pub async fn landing_of(repo: &RepoStore, version: &ChangeVersionHash) -> Result<Option<Landing>> {
    Ok(repo.landing_of(version).await?)
}

/// The most recent landings, oldest first, up to [`RECENT_LANDINGS`].
pub async fn list_landings(repo: &RepoStore) -> Result<Vec<Landing>> {
    Ok(repo.list_landings(RECENT_LANDINGS).await?)
}
