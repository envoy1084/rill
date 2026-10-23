//! Eager mounts: the base written to disk, edits found by a filesystem
//! watcher and reconciled into the overlay.

mod disk;
mod ignore;
mod materialize;
mod reconcile;
mod watch;

pub use self::ignore::{IgnoreRules, IGNORE_FILE, MARKER};
pub(crate) use materialize::{apply_disk, materialize, DiskOp};
pub use watch::Watch;
