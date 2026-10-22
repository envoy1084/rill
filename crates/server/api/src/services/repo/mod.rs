//! `rill.v1.RepoService`: repositories, refs, releases, trunk positions
//! and their watches. The trait impl only dispatches; each group of RPCs
//! lives in its own file.

mod refs;
mod repos;
mod service;
mod watch;

pub use service::RepoSvc;
pub use watch::EventStream;
pub(crate) use watch::{forward, start_from};
