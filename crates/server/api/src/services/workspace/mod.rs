//! `rill.v1.WorkspaceService`: server-side working copies with an
//! operation log.
//!
//! A workspace's `base` is `ObjectHash::ZERO` while the repository has no
//! history; every other base is a change version. Each mutating RPC
//! appends one [`Operation`](rill_types::Operation) holding the whole
//! workspace afterwards, which is what `Undo` restores; undoing an `Undo`
//! is a redo.

mod changes;
mod history;
mod lifecycle;
mod oplog;
mod service;

pub use service::WorkspaceSvc;
