//! Keeping each mount's server workspace in step with its overlay.

mod push;
pub(crate) mod task;
mod upload;

pub(crate) use upload::stored_edits;
