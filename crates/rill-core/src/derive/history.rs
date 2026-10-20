//! Path history: for every version, a tree of "last changed" links.
//!
//! The value for a version is the hash of a [`HistoryNode`] for the root
//! directory. Each entry records the most recent version (along the
//! first-parent line) that changed the entry's path, and for directories
//! the node describing their children. Deriving a version rewrites only
//! the nodes on the paths it touched, sharing everything else with its
//! first parent, so the cost is proportional to the change.
//!
//! `history(path, from, limit)` follows the links: the node at `from`
//! names the last version `v` that touched `path`; the node at `v`'s first
//! parent names the one before, and so on. Changes reachable only through
//! second parents of merges are not visited, which matches how file
//! changes are stored (relative to the first parent).

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use rill_store::ObjectStoreExt;
use rill_types::{
    ChangeVersion, ChangeVersionHash, Encodable, ObjectHash, ObjectKind, RepoPath, Schema,
};
use serde::{Deserialize, Serialize};

use super::ctx::DeriveCtx;
use super::manager::DeriveManager;
use super::traits::Derivable;
use crate::error::{Error, Result};
use crate::tree::TreeOps;

/// One entry of a [`HistoryNode`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Last version that changed this path or anything beneath it.
    pub version: ChangeVersionHash,
    /// Child node for a directory; `None` for files and for directories
    /// replaced wholesale by a subtree copy.
    pub subtree: Option<ObjectHash>,
}

/// History links for the entries of one directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryNode {
    pub entries: BTreeMap<String, HistoryEntry>,
}

impl Encodable for HistoryNode {
    const KIND: ObjectKind = ObjectKind::Derived;
    const SCHEMA: Schema = 1;
    const NAME: &'static str = "HistoryNode";
}

/// Root history node hash of a version.
pub struct PathHistory;

#[async_trait]
impl Derivable for PathHistory {
    const KIND: &'static str = "phist";
    const VERSION: u8 = 1;
    type Value = ObjectHash;

    async fn derive(
        ctx: &DeriveCtx,
        cv: &ChangeVersion,
        parents: Vec<ObjectHash>,
    ) -> Result<ObjectHash> {
        let version = cv.version_hash();
        let mut trie = Trie::default();
        for path in cv.subtree_changes.keys() {
            trie.insert(path, true);
        }
        for path in cv.file_changes.keys() {
            trie.insert(path, false);
        }
        let base = parents.first().copied();
        write_node(ctx.trees(), base, trie, version).await
    }
}

#[derive(Default)]
struct Trie {
    /// Set when this exact path was changed; `true` for subtree changes,
    /// whose children are no longer described.
    touched: Option<bool>,
    children: BTreeMap<String, Trie>,
}

impl Trie {
    fn insert(&mut self, path: &RepoPath, subtree: bool) {
        let mut node = self;
        for c in path.components() {
            node = node.children.entry(c.to_string()).or_default();
        }
        node.touched = Some(subtree);
    }
}

fn write_node(
    trees: &TreeOps,
    base: Option<ObjectHash>,
    trie: Trie,
    version: ChangeVersionHash,
) -> Pin<Box<dyn Future<Output = Result<ObjectHash>> + Send + '_>> {
    Box::pin(async move {
        let mut node = match base {
            Some(h) => load_node(trees, &h).await?,
            None => HistoryNode::default(),
        };
        for (name, child) in trie.children {
            let existing = node.entries.get(&name).and_then(|e| e.subtree);
            let subtree = if child.children.is_empty() {
                // A file change, or a whole-directory change: no children
                // to describe (for a subtree copy the children's history
                // starts here).
                if child.touched == Some(true) {
                    None
                } else {
                    existing
                }
            } else {
                let base = if child.touched == Some(true) {
                    None
                } else {
                    existing
                };
                Some(write_node(trees, base, child, version).await?)
            };
            node.entries.insert(name, HistoryEntry { version, subtree });
        }
        Ok(trees.objects().put_object(&node).await?)
    })
}

async fn load_node(trees: &TreeOps, hash: &ObjectHash) -> Result<HistoryNode> {
    trees
        .objects()
        .get_object(hash)
        .await?
        .ok_or_else(|| Error::Derive {
            kind: PathHistory::KIND,
            hash: *hash,
            reason: "history node missing from object store".into(),
        })
}

/// Last version at or before the version owning `root` that changed
/// `path`, following the history nodes.
async fn last_changed(
    trees: &TreeOps,
    root: ObjectHash,
    path: &RepoPath,
) -> Result<Option<ChangeVersionHash>> {
    let mut current = root;
    let mut components = path.components().peekable();
    while let Some(name) = components.next() {
        let node = load_node(trees, &current).await?;
        let Some(entry) = node.entries.get(name) else {
            return Ok(None);
        };
        if components.peek().is_none() {
            return Ok(Some(entry.version));
        }
        match entry.subtree {
            Some(h) => current = h,
            // Below a wholesale-replaced directory everything changed in
            // that version.
            None => return Ok(Some(entry.version)),
        }
    }
    Ok(None)
}

impl DeriveManager {
    /// Versions that changed `path`, newest first, starting at `from`.
    ///
    /// The root path yields the first-parent line itself.
    pub async fn history(
        self: &std::sync::Arc<Self>,
        path: &RepoPath,
        from: &ChangeVersionHash,
        limit: usize,
    ) -> Result<Vec<ChangeVersionHash>> {
        let mut out = Vec::new();
        let mut cursor = Some(*from);
        while let Some(at) = cursor {
            if out.len() >= limit {
                break;
            }
            let hit = if path.is_root() {
                Some(at)
            } else {
                let root = self.get::<PathHistory>(&at).await?;
                last_changed(self.trees(), root, path).await?
            };
            let Some(hit) = hit else { break };
            out.push(hit);
            cursor = self.change(&hit).await?.first_parent();
        }
        Ok(out)
    }
}
