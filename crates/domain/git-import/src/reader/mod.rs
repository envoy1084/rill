//! Reading git repositories.

mod gix_source;
mod history;
mod memory;
mod source;

pub use gix_source::GixSource;
pub use history::history;
pub use memory::MemorySource;
pub use source::{CommitChanges, GitChange, GitCommit, GitSha, GitSource};
