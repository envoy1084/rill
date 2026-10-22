//! Comment threads and their anchors.

mod anchor;
mod comment;

pub use anchor::{context_hash, find_context, CONTEXT_LINES};
pub use comment::{new_thread_id, CommentMeta};
