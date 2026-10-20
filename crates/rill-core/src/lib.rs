//! Core algorithms of Rill: diff, merge, trees, ancestry, derived data,
//! rebase and blob storage.
//!
//! Everything here is a pure function of objects in a [`rill_store`]; no
//! service state, no RPC.

pub mod blobs;
pub mod builder;
pub mod derive;
pub mod diff;
pub mod error;
pub mod graph;
pub mod merge;
pub mod rebase;
pub mod tree;
mod util;
pub mod validate;

pub use blobs::{read_content, store_content, BlobReader};
pub use builder::ChangeBuilder;
pub use derive::{
    Blame, BlameLine, ChangeSummary, Derivable, DeriveCtx, DeriveManager, Generation, PathHistory,
    RootTree,
};
pub use diff::{
    apply_unified, diff_lines, diff_trees, is_binary, to_unified, Hunk, LineTag, TreeDiffEntry,
};
pub use error::{Error, Result};
pub use graph::{CommitGraph, LogEntry};
pub use merge::{
    materialize, merge_changes, merge_file, parse_materialized, FileMergeResult,
    MergeChangeOptions, MergeHunk,
};
pub use rebase::rebase;
pub use tree::{TreeBuilder, TreeOps};
pub use validate::{validate_version, PathRules};
