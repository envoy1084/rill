//! Minimal working-copy support: materialize a root into a directory,
//! track it in `.rill/`, report local changes, snapshot them to the
//! server and update to a new root without losing local work.

mod checkout;
mod fs;
mod meta;
mod remote;
mod status;
mod update;
mod walk;

pub use checkout::Checkout;
pub use meta::{CheckoutMeta, Manifest, ManifestEntry, META_DIR};
pub use status::{LocalChange, LocalChangeKind};
pub use update::UpdateReport;
pub use walk::IGNORE_FILE;
