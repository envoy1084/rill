//! Whether trunk may move to a version secret scanning held back.

use rill_core::CommitGraph;
use rill_store::RepoStore;
use rill_types::ChangeVersionHash;

use crate::error::Result;

/// Why trunk may not move to `version`: it, or an ancestor of it not yet
/// on trunk, is quarantined. Moving trunk brings in the whole history, so
/// every such ancestor counts. Quarantines are few, so each is tested
/// against `version`'s ancestry rather than walking history.
pub async fn held_back(
    repo: &RepoStore,
    graph: &CommitGraph,
    version: &ChangeVersionHash,
) -> Result<Option<String>> {
    for (held, why) in repo.quarantined_versions().await? {
        if repo.trunk_seq_of(&held).await?.is_some() || !graph.is_ancestor(&held, version).await? {
            continue;
        }
        return Ok(Some(if held == *version {
            why
        } else {
            format!("its ancestor {held} is quarantined: {why}")
        }));
    }
    Ok(None)
}
