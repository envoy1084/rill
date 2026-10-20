//! One-way import of git history into a Rill repository.
//!
//! Each git commit becomes a [`rill_types::ChangeVersion`] whose
//! `change_id` is derived from the sha, whose file changes are the diff
//! against the first parent, and whose `extra["git_sha"]` keeps the
//! original id. The mapping from sha to version is persisted so imports
//! are incremental: pulling new commits into git and importing again only
//! converts what is new. The mainline (first-parent ancestry of the head)
//! is landed on trunk in order so sequence numbers mean what they do for
//! native history.
//!
//! Reading goes through the [`reader::GitSource`] trait; [`reader::GixSource`]
//! reads a repository on disk with `gix` and [`reader::MemorySource`] is for
//! tests. There is no reverse direction and no git compatibility beyond
//! this.

pub mod error;
pub mod import;
pub mod reader;

pub use error::{Error, Result};
pub use import::{ImportMap, ImportOptions, ImportReport, Importer};
pub use reader::{GitChange, GitCommit, GitSha, GitSource, GixSource, MemorySource};
