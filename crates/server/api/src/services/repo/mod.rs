//! `rill.v1.RepoService`: history, trees, blobs, diffs, writes, refs and
//! watches. The trait impl only dispatches; each group of RPCs lives in
//! its own file.

pub(crate) mod authorize;
mod commit;
mod grep;
mod history;
mod refs;
mod service;
mod stamp;
mod trees;
pub(crate) mod visible;
mod watch;
mod write;

pub use commit::{commit_version, commit_version_with, CommitExtras};
pub use service::RepoSvc;
pub use trees::BlobStream;
pub(crate) use watch::forward;
pub use watch::EventStream;
pub use write::version_from_diff;
