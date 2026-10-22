//! The process-wide FoundationDB client network.

use std::sync::OnceLock;

use foundationdb::api::NetworkAutoStop;
use foundationdb::Database;

use crate::error::{Error, Result};

static NETWORK: OnceLock<NetworkAutoStop> = OnceLock::new();

/// Open a database, starting the client network on first use.
pub fn open_database(cluster_file: Option<&str>) -> Result<Database> {
    NETWORK.get_or_init(|| {
        // SAFETY: `boot` may run once per process and the network must
        // outlive every database. The guard lives in a static that is
        // never dropped, so the network runs until the process exits.
        unsafe { foundationdb::boot() }
    });
    Database::new(cluster_file)
        .map_err(|e| Error::Backend(format!("opening foundationdb: {}", e.message())))
}
