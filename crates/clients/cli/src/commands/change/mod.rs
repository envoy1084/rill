//! Working-change commands on a checkout: new, describe, amend, sync,
//! resolve, undo.
//!
//! A checkout tracks one *working change*. `new` snapshots local edits
//! into a change (stacking on top of the previous working change when
//! there is one), `amend` and `describe` re-snapshot into it, `sync` moves
//! the base and merges local edits, `resolve` clears conflict flags after
//! the user edits the markers, and `undo` reverts the server workspace's
//! last operation and restores its base locally.

mod args;
mod render;
mod resolve;
mod snapshot;
mod sync;
mod undo;

pub use args::{AmendArgs, DescribeArgs, NewArgs, ResolveArgs, SyncArgs};
pub(crate) use render::conflicts_of;
pub use resolve::{conflicts, resolve};
pub use snapshot::{amend, describe, new};
pub use sync::sync;
pub(crate) use sync::update_checkout;
pub use undo::undo;
