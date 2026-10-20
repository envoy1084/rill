//! Tree node reads and writes over an object store with a decoded-node
//! cache.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use rill_store::{ObjectStore, ObjectStoreExt};
use rill_types::{
    Encodable, EntryKind, ObjectHash, ObjectKind, RepoPath, Tree, TreeChild, TreeHash, TreeNode,
};

use crate::error::{Error, Result};
use crate::util::LruCache;

/// Default number of decoded tree nodes kept in memory.
pub const DEFAULT_TREE_CACHE: usize = 4096;

/// A decoded tree node, cheap to clone.
#[derive(Debug, Clone)]
pub enum NodeRef {
    Leaf(Arc<Tree>),
    Inner {
        level: u8,
        children: Arc<[TreeChild]>,
        names: Option<TreeHash>,
    },
    Names(Arc<[String]>),
}

impl From<TreeNode> for NodeRef {
    fn from(node: TreeNode) -> Self {
        match node {
            TreeNode::Leaf(tree) => NodeRef::Leaf(Arc::new(tree)),
            TreeNode::Inner {
                level,
                children,
                names,
            } => NodeRef::Inner {
                level,
                children: children.into(),
                names,
            },
            TreeNode::Names(names) => NodeRef::Names(names.into()),
        }
    }
}

/// Tree operations bound to an object store.
#[derive(Clone)]
pub struct TreeOps {
    objects: Arc<dyn ObjectStore>,
    cache: Arc<LruCache<TreeHash, NodeRef>>,
    written: Arc<AtomicU64>,
}

impl TreeOps {
    /// Create tree operations with a cache of `cache_size` decoded nodes.
    pub fn new(objects: Arc<dyn ObjectStore>, cache_size: usize) -> Self {
        TreeOps {
            objects,
            cache: Arc::new(LruCache::new(cache_size)),
            written: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The underlying object store.
    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        &self.objects
    }

    /// Tree nodes these operations have written to the store.
    pub fn nodes_written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// Fetch and decode one node, consulting the cache first.
    pub async fn get_node(&self, hash: &TreeHash) -> Result<NodeRef> {
        if let Some(node) = self.cache.get(hash) {
            return Ok(node);
        }
        let node: TreeNode = self
            .objects
            .get_object(hash)
            .await?
            .ok_or(Error::TreeNotFound(*hash))?;
        let node = NodeRef::from(node);
        self.cache.put(*hash, node.clone());
        Ok(node)
    }

    /// Store one node. A node already in the cache exists in the store, so
    /// it is not written again.
    pub(crate) async fn put_node(&self, node: TreeNode) -> Result<TreeHash> {
        let bytes = node.encode();
        let hash = ObjectHash::digest(ObjectKind::Tree, &bytes);
        if self.cache.get(&hash).is_some() {
            return Ok(hash);
        }
        self.objects
            .put(ObjectKind::Tree, &hash, Bytes::from(bytes))
            .await?;
        self.written.fetch_add(1, Ordering::Relaxed);
        self.cache.put(hash, NodeRef::from(node));
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
            return Ok(Some(EntryKind::Dir {
                hash: root,
                summary: self.summary(&root).await?,
            }));
        }
        while let Some(name) = components.next() {
            let Some(entry) = self.entry(&current, name).await? else {
                return Ok(None);
            };
            if components.peek().is_none() {
                return Ok(Some(entry.kind));
            }
            match entry.kind {
                EntryKind::Dir { hash, .. } => current = hash,
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
