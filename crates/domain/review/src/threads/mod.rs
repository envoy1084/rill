//! Comment threads and their anchors.

mod anchor;
mod comment;
mod suggest;

pub use anchor::{context_hash, find_context, CONTEXT_LINES};
pub use comment::{new_thread_id, CommentMeta};
pub use suggest::{replace_lines, SuggestedEdit};
