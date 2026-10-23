//! Trees by hash: in memory, then in the object cache, then from the
//! server. Trees are immutable, so a cached one never goes stale.

use std::sync::Arc;

use rill_sdk::RepoClient;
use rill_types::{Tree, TreeHash};

use super::store::ObjectCache;
use crate::error::Result;

/// Most trees kept in memory.
const MEMORY_TREES: u64 = 200_000;

/// The tree cache shared by every mount.
#[derive(Clone, Debug)]
pub struct TreeCache {
    memory: moka::sync::Cache<TreeHash, Arc<Tree>>,
    disk: Arc<ObjectCache>,
}

impl TreeCache {
    /// A tree cache spilling to `disk`.
    pub fn new(disk: Arc<ObjectCache>) -> Self {
        TreeCache {
            memory: moka::sync::Cache::new(MEMORY_TREES),
            disk,
        }
    }

    /// The tree with `hash`, fetched from `repo` on a miss.
    pub async fn get(&self, repo: &RepoClient, hash: TreeHash) -> Result<Arc<Tree>> {
        if let Some(tree) = self.memory.get(&hash) {
            return Ok(tree);
        }
        let disk = self.disk.clone();
        let stored = tokio::task::spawn_blocking(move || disk.get(&hash)).await??;
        let tree = match stored.and_then(|b| postcard::from_bytes::<Tree>(&b).ok()) {
            Some(tree) => Arc::new(tree),
            None => {
                let tree = Arc::new(repo.tree(hash).await?);
                let bytes = postcard::to_stdvec(tree.as_ref())?;
                let disk = self.disk.clone();
                tokio::task::spawn_blocking(move || disk.put(&hash, &bytes, false)).await??;
                tree
            }
        };
        self.memory.insert(hash, tree.clone());
        Ok(tree)
    }
}
