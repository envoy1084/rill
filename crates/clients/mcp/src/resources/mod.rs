//! MCP resources: files, change diffs and reviews addressable by URI,
//! with update notifications for the change resources.

mod follow;
mod read;
mod uri;

pub use follow::{updates, watchable};
pub use read::{listed, read, templates};
pub use uri::ResourceUri;
