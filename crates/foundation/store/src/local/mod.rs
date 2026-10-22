//! Local on-disk backends: objects as files, metadata in a redb database.
//!
//! This is the single-node deployment: everything under one directory,
//! no services to run. [`LocalStores::open`] wires both halves together.

mod meta;
mod stores;

pub use meta::{RedbBatch, RedbMetaStore};
pub use stores::LocalStores;
