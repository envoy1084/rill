//! Shared context handed to derivations.

use std::sync::Arc;

use async_trait::async_trait;
use rill_store::{ObjectStore, RepoStore};
use rill_types::{ChangeVersionHash, TreeHash};

use super::manager::DeriveManager;
use super::root_tree::RootTree;
use crate::error::Result;
use crate::tree::{RootLookup, TreeOps};

/// Everything a derivation may read: the repository, tree operations and
/// the manager itself for cross-derivable lookups.
#[derive(Clone)]
pub struct DeriveCtx {
    repo: RepoStore,
    trees: TreeOps,
    manager: Arc<DeriveManager>,
}

impl DeriveCtx {
    pub(crate) fn new(repo: RepoStore, trees: TreeOps, manager: Arc<DeriveManager>) -> Self {
        DeriveCtx {
            repo,
            trees,
            manager,
        }
    }

    /// The repository.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// Tree operations over the repository's object store.
    pub fn trees(&self) -> &TreeOps {
        &self.trees
    }

    /// The object store.
    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        self.repo.objects()
    }

    /// The manager, for derivations that depend on other derivables.
    pub fn manager(&self) -> &Arc<DeriveManager> {
        &self.manager
    }
}

#[async_trait]
impl RootLookup for DeriveCtx {
    async fn root_tree(&self, version: &ChangeVersionHash) -> Result<TreeHash> {
        self.manager.get::<RootTree>(version).await
    }
}
