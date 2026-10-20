//! Three-way merging: text, conflict materialization and file states.

mod base;
pub mod change;
mod file;
mod materialize;
mod parse;
pub mod state;

pub use change::{merge_changes, MergeChangeOptions};
pub use file::{merge_file, FileMergeResult, MergeHunk};
pub use materialize::{materialize, ConflictLabels};
pub use parse::parse_materialized;
pub use state::{merge_file_state, merge_states};
