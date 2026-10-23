//! Mounts: working copies the daemon serves, with their overlay, base and
//! the operations on them (status, commit, update, resolve).

mod commit;
mod merge;
#[allow(clippy::module_inception)]
mod mount;
mod record;
mod resolve;
mod status;
mod update;

pub use commit::{CommitOptions, Committed};
pub use mount::{Change, Mount, Settler};
pub use record::{MountMode, MountRecord};
pub(crate) use update::resolve_base;
pub use update::UpdateReport;
