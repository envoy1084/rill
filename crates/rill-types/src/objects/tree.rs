//! Directory trees.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::encoding::{Encodable, FormatVersion};
use crate::hash::{ObjectHash, ObjectKind};
use crate::objects::blob::ContentRef;
use crate::objects::change::FileKind;

/// Hash of a [`Tree`].
pub type TreeHash = ObjectHash;

/// What a directory entry points at.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryKind {
    File { content: ContentRef, kind: FileKind },
    Dir { hash: TreeHash, summary: DirSummary },
}

/// Aggregate facts about a subtree, stored on the parent entry so that
/// size and count queries never need to walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct DirSummary {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
}

/// A directory: its entries sorted by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Tree {
    pub entries: BTreeMap<String, TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeEntry {
    pub kind: EntryKind,
}

impl Tree {
    pub fn empty() -> Self {
        Tree::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&TreeEntry> {
        self.entries.get(name)
    }

    pub fn insert_file(&mut self, name: impl Into<String>, content: ContentRef, kind: FileKind) {
        self.entries.insert(
            name.into(),
            TreeEntry {
                kind: EntryKind::File { content, kind },
            },
        );
    }

    pub fn insert_dir(&mut self, name: impl Into<String>, hash: TreeHash, summary: DirSummary) {
        self.entries.insert(
            name.into(),
            TreeEntry {
                kind: EntryKind::Dir { hash, summary },
            },
        );
    }

    pub fn remove(&mut self, name: &str) -> Option<TreeEntry> {
        self.entries.remove(name)
    }

    /// Aggregate summary of this tree, suitable for storing on the parent
    /// entry that points at it.
    pub fn summary(&self) -> DirSummary {
        let mut s = DirSummary::default();
        for entry in self.entries.values() {
            match &entry.kind {
                EntryKind::File { content, .. } => {
                    s.file_count += 1;
                    s.total_size += content.size();
                }
                EntryKind::Dir { summary, .. } => {
                    s.dir_count += 1 + summary.dir_count;
                    s.file_count += summary.file_count;
                    s.total_size += summary.total_size;
                }
            }
        }
        s
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &ContentRef, FileKind)> {
        self.entries.iter().filter_map(|(name, e)| match &e.kind {
            EntryKind::File { content, kind } => Some((name.as_str(), content, *kind)),
            EntryKind::Dir { .. } => None,
        })
    }

    pub fn dirs(&self) -> impl Iterator<Item = (&str, TreeHash)> {
        self.entries.iter().filter_map(|(name, e)| match &e.kind {
            EntryKind::Dir { hash, .. } => Some((name.as_str(), *hash)),
            EntryKind::File { .. } => None,
        })
    }
}

impl Encodable for Tree {
    const KIND: ObjectKind = ObjectKind::Tree;
    const VERSION: FormatVersion = 1;
    const NAME: &'static str = "Tree";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_hash_is_stable() {
        assert_eq!(Tree::empty().hash(), Tree::empty().hash());
    }

    #[test]
    fn summary_aggregates() {
        let mut inner = Tree::empty();
        inner.insert_file("a", ContentRef::inline(b"xyz".to_vec()), FileKind::Regular);
        let mut outer = Tree::empty();
        outer.insert_dir("d", inner.hash(), inner.summary());
        outer.insert_file("b", ContentRef::inline(b"12".to_vec()), FileKind::Regular);
        let s = outer.summary();
        assert_eq!(s.file_count, 2);
        assert_eq!(s.dir_count, 1);
        assert_eq!(s.total_size, 5);
    }
}
