//! The FoundationDB metadata store (feature `fdb`).

mod engine;
mod meta_store;
mod network;

pub use engine::{FdbEngine, FdbTxn};
pub use meta_store::FdbMetaStore;
pub use network::open_database;
