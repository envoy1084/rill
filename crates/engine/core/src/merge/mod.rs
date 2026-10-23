//! Three-way merging: text, conflict materialization and file states.

mod base;
pub mod change;
mod content;
mod file;
mod materialize;
mod parse;
pub mod state;

pub use change::{merge_changes, MergeChangeOptions};
pub use content::{record_structural, ContentMerger, Structural, STRUCTURAL_MERGE_EXTRA};
pub use file::{merge_file, FileMergeResult, MergeHunk};
pub use materialize::{materialize, ConflictLabels};
pub use parse::{parse_materialized, parse_regions, Region};
pub use state::{merge_file_state, merge_file_state_with, merge_states, Merged};
