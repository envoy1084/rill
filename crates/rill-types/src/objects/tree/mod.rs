//! Directory trees.
//!
//! A directory is a [`Tree`]: its entries sorted by name. It is stored as
//! one [`TreeNode`] when it has at most [`SHARD_ENTRIES`] entries, and
//! otherwise as a balanced tree of nodes whose boundaries depend only on
//! the entry names, so equal directories always have equal hashes.

mod listing;
mod node;
pub mod shape;

pub use listing::{DirSummary, EntryKind, Tree, TreeEntry, TreeHash};
pub use node::{TreeChild, TreeNode};
pub use shape::SHARD_ENTRIES;
