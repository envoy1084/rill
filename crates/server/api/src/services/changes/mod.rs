//! `rill.v1.ChangeService`: reading versions and writing new ones. The
//! trait impl only dispatches; reads, writes and the commit path that
//! every write shares live in their own files.

pub(crate) mod authorize;
mod commit;
mod service;
mod stamp;
mod versions;
pub(crate) mod visible;
mod write;

pub use commit::{commit_version, commit_version_with, CommitExtras};
pub use service::ChangeSvc;
pub use write::version_from_diff;
