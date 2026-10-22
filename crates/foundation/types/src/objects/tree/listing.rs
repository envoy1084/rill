//! A directory's entries, and the summaries parents keep of them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::encoding::Encodable;
use crate::hash::ObjectHash;
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
/// size, count and depth queries never need to walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct DirSummary {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    /// Levels of directories below this one: 0 when it holds only files.
    pub max_depth: u32,
}

impl DirSummary {
    /// The summary of two disjoint sets of entries of one directory.
    pub fn merge(self, other: DirSummary) -> DirSummary {
        DirSummary {
            file_count: self.file_count + other.file_count,
            dir_count: self.dir_count + other.dir_count,
            total_size: self.total_size + other.total_size,
            max_depth: self.max_depth.max(other.max_depth),
        }
    }

    /// What one entry contributes to its directory's summary.
    pub fn of_entry(kind: &EntryKind) -> DirSummary {
        match kind {
            EntryKind::File { content, .. } => DirSummary {
                file_count: 1,
                total_size: content.size(),
                ..DirSummary::default()
            },
            EntryKind::Dir { summary, .. } => DirSummary {
                file_count: summary.file_count,
                dir_count: 1 + summary.dir_count,
                total_size: summary.total_size,
                max_depth: 1 + summary.max_depth,
            },
        }
    }
}

/// A directory: its entries sorted by name. This is the logical listing;
/// how it is stored is [`TreeNode`](super::TreeNode)'s concern.
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
        self.entries
            .values()
            .map(|e| DirSummary::of_entry(&e.kind))
            .fold(DirSummary::default(), DirSummary::merge)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The hash of the empty directory.
    pub fn empty_hash() -> TreeHash {
        super::TreeNode::Leaf(Tree::empty()).hash()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_aggregates() {
        let mut inner = Tree::empty();
        inner.insert_file("a", ContentRef::inline(b"xyz".to_vec()), FileKind::Regular);
        let mut outer = Tree::empty();
        outer.insert_dir("d", Tree::empty_hash(), inner.summary());
        outer.insert_file("b", ContentRef::inline(b"12".to_vec()), FileKind::Regular);
        let s = outer.summary();
        assert_eq!(s.file_count, 2);
        assert_eq!(s.dir_count, 1);
        assert_eq!(s.total_size, 5);
        assert_eq!(s.max_depth, 1);
    }
}
