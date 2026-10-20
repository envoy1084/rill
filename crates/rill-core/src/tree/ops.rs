//! Tree reads and writes over an object store with a decoded-tree cache.

use std::sync::Arc;

use rill_store::{ObjectStore, ObjectStoreExt};
use rill_types::{EntryKind, RepoPath, Tree, TreeHash};

use crate::error::{Error, Result};
use crate::util::LruCache;

/// Default number of decoded trees kept in memory.
pub const DEFAULT_TREE_CACHE: usize = 4096;

/// Tree operations bound to an object store.
#[derive(Clone)]
pub struct TreeOps {
    objects: Arc<dyn ObjectStore>,
    cache: Arc<LruCache<TreeHash, Arc<Tree>>>,
}

impl TreeOps {
    /// Create tree operations with a cache of `cache_size` decoded trees.
    pub fn new(objects: Arc<dyn ObjectStore>, cache_size: usize) -> Self {
        TreeOps {
            objects,
            cache: Arc::new(LruCache::new(cache_size)),
        }
    }

    /// The underlying object store.
    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        &self.objects
    }

    /// Fetch and decode a tree, consulting the cache first.
    pub async fn get_tree(&self, hash: &TreeHash) -> Result<Arc<Tree>> {
        if let Some(tree) = self.cache.get(hash) {
            return Ok(tree);
        }
        let tree: Tree = self
            .objects
            .get_object(hash)
            .await?
            .ok_or(Error::TreeNotFound(*hash))?;
        let tree = Arc::new(tree);
        self.cache.put(*hash, tree.clone());
        Ok(tree)
    }

    /// Store a tree and return its hash.
    pub async fn put_tree(&self, tree: &Tree) -> Result<TreeHash> {
        let hash = self.objects.put_object(tree).await?;
        self.cache.put(hash, Arc::new(tree.clone()));
        Ok(hash)
    }

    /// Hash of the empty tree, stored so it can always be loaded.
    pub async fn empty_tree(&self) -> Result<TreeHash> {
        self.put_tree(&Tree::empty()).await
    }

    /// Find the entry at `path` under `root`; the root path yields a `Dir`.
    pub async fn lookup(&self, root: TreeHash, path: &RepoPath) -> Result<Option<EntryKind>> {
        let mut current = root;
        let mut components = path.components().peekable();
        if components.peek().is_none() {
            let tree = self.get_tree(&root).await?;
            return Ok(Some(EntryKind::Dir {
                hash: root,
                summary: tree.summary(),
            }));
        }
        while let Some(name) = components.next() {
            let tree = self.get_tree(&current).await?;
            let Some(entry) = tree.get(name) else {
                return Ok(None);
            };
            if components.peek().is_none() {
                return Ok(Some(entry.kind.clone()));
            }
            match &entry.kind {
                EntryKind::Dir { hash, .. } => current = *hash,
                EntryKind::File { .. } => return Ok(None),
            }
        }
        Ok(None)
    }

    /// Hash of the directory at `path`, or `None` if it is absent or a file.
    pub async fn dir_hash(&self, root: TreeHash, path: &RepoPath) -> Result<Option<TreeHash>> {
        Ok(match self.lookup(root, path).await? {
            Some(EntryKind::Dir { hash, .. }) => Some(hash),
            _ => None,
        })
    }

    /// Entries of the directory at `path`, sorted by name.
    pub async fn list_dir(
        &self,
        root: TreeHash,
        path: &RepoPath,
    ) -> Result<Vec<(String, EntryKind)>> {
        let hash = self
            .dir_hash(root, path)
            .await?
            .ok_or_else(|| Error::NotADirectory(path.to_owned()))?;
        let tree = self.get_tree(&hash).await?;
        Ok(tree
            .entries
            .iter()
            .map(|(name, e)| (name.clone(), e.kind.clone()))
            .collect())
    }
}
