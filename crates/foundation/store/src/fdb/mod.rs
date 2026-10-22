//! The FoundationDB metadata store (feature `fdb`).

mod engine;
mod network;

pub use engine::{FdbEngine, FdbTxn};
pub use network::open_database;

use crate::error::Result;
use crate::keyspace::Keyspace;

/// The FoundationDB metadata store.
pub type FdbMetaStore = Keyspace<FdbEngine>;

impl Keyspace<FdbEngine> {
    /// Connect through `cluster_file` (the platform default when `None`)
    /// and keep every key in the subspace named `root`.
    pub fn connect(cluster_file: Option<&str>, root: &str) -> Result<Self> {
        let db = open_database(cluster_file)?;
        let root = foundationdb_tuple::pack(&(root,));
        Ok(Keyspace::with_engine(FdbEngine::new(db, &root)?))
    }
}
