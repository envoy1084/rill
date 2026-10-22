//! Tree node reads and writes over an object store with a decoded-node
//! cache.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use rill_store::{ObjectStore, ObjectStoreExt};
use rill_types::{
    Encodable, EntryKind, ObjectHash, ObjectKind, RepoPath, Tree, TreeChild, TreeHash, TreeNode,
};

use crate::error::{Error, Result};
use moka::sync::Cache;

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

/// Nodes a build wrote but has not stored yet, by hash.
type Pending = Arc<parking_lot::Mutex<HashMap<TreeHash, (Bytes, NodeRef)>>>;

/// Node writes a build keeps in flight at once when it stores them.
const WRITE_CONCURRENCY: usize = 16;

/// Tree operations bound to an object store.
#[derive(Clone)]
pub struct TreeOps {
    objects: Arc<dyn ObjectStore>,
    cache: Arc<Cache<TreeHash, NodeRef>>,
    written: Arc<AtomicU64>,
    /// While set, new nodes wait here and are stored together by
    /// [`TreeOps::flush`].
    pending: Option<Pending>,
}

impl TreeOps {
    /// Create tree operations with a cache of `cache_size` decoded nodes.
    pub fn new(objects: Arc<dyn ObjectStore>, cache_size: usize) -> Self {
        TreeOps {
            objects,
            cache: Arc::new(Cache::new(cache_size as u64)),
            written: Arc::new(AtomicU64::new(0)),
            pending: None,
        }
    }

    /// The same operations, holding new nodes back until
    /// [`TreeOps::flush`]: a build's nodes are then written concurrently
    /// instead of one after another, children before parents. Reads see
    /// the held nodes.
    pub(crate) fn deferring(&self) -> TreeOps {
        TreeOps {
            pending: Some(Default::default()),
            ..self.clone()
        }
    }

    /// Whether new nodes are held back for a later [`TreeOps::flush`].
    pub(crate) fn is_deferring(&self) -> bool {
        self.pending.is_some()
    }

    /// Store the nodes held back since [`TreeOps::deferring`].
    pub(crate) async fn flush(&self) -> Result<()> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        let nodes: Vec<(TreeHash, (Bytes, NodeRef))> = pending.lock().drain().collect();
        let stored = nodes.len() as u64;
        futures::stream::iter(nodes)
            .map(|(hash, (bytes, node))| async move {
                self.objects.put(ObjectKind::Tree, &hash, bytes).await?;
                self.cache.insert(hash, node);
                Ok::<_, Error>(())
            })
            .buffer_unordered(WRITE_CONCURRENCY)
            .try_collect::<Vec<()>>()
            .await?;
        self.written.fetch_add(stored, Ordering::Relaxed);
        Ok(())
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
        if let Some((_, node)) = self
            .pending
            .as_ref()
            .and_then(|p| p.lock().get(hash).cloned())
        {
            return Ok(node);
        }
        if let Some(node) = self.cache.get(hash) {
            self.objects.refuses(ObjectKind::Tree, hash)?;
            return Ok(node);
        }
        let node: TreeNode = self
            .objects
            .get_object(hash)
            .await?
            .ok_or(Error::TreeNotFound(*hash))?;
        let node = NodeRef::from(node);
        self.cache.insert(*hash, node.clone());
        Ok(node)
    }

    /// Store one node. A node already in the cache exists in the store, so
    /// it is not written again.
    pub(crate) async fn put_node(&self, node: TreeNode) -> Result<TreeHash> {
        let bytes = node.encode();
        let hash = ObjectHash::digest(ObjectKind::Tree, &bytes);
        if self.cache.get(&hash).is_some() {
            self.objects.refuses(ObjectKind::Tree, &hash)?;
            return Ok(hash);
        }
        if let Some(pending) = &self.pending {
            pending
                .lock()
                .insert(hash, (Bytes::from(bytes), NodeRef::from(node)));
            return Ok(hash);
        }
        self.objects
            .put(ObjectKind::Tree, &hash, Bytes::from(bytes))
            .await?;
        self.written.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(hash, NodeRef::from(node));
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
