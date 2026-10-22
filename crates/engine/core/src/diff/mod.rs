//! Line diffs, unified rendering, binary detection and tree diffs.

mod binary;
mod hunk;
pub(crate) mod lines;
mod patch;
pub(crate) mod tokens;
pub mod tree;
mod unified;

pub use binary::{is_binary, BINARY_SNIFF_LEN};
pub use hunk::{DiffLine, Hunk, LineTag};
pub use lines::{diff_lines, diff_lines_with_context, DEFAULT_CONTEXT};
pub use patch::apply_unified;
pub use tokens::{line_content, split_lines, LineTokens};
pub use tree::{diff_trees, file_state, TreeDiffEntry};
pub use unified::to_unified;
