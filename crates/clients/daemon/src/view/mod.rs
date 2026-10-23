//! Read views of a mount's base tree.

mod base;
mod diff;
mod sparse;

pub use base::{Base, BaseEntry};
pub use diff::{diff_trees, FileState, TreeChange};
pub use sparse::dir_may_contain;
