//! What every mount shares: the state database, the caches and the
//! server clients.

use std::sync::Arc;

use crate::cache::{Content, ObjectCache, TreeCache};
use crate::clients::Clients;
use crate::config::DaemonConfig;
use crate::error::Result;
use crate::state::StateDb;

/// The daemon-wide context.
pub struct Shared {
    /// Durable state.
    pub db: Arc<StateDb>,
    /// File contents.
    pub content: Content,
    /// Trees.
    pub trees: TreeCache,
    /// Server clients.
    pub clients: Clients,
    /// Configuration.
    pub config: DaemonConfig,
}

impl Shared {
    /// Open the state and caches under the configured directory.
    pub fn open(config: DaemonConfig) -> Result<Self> {
        let dir = &config.dir;
        std::fs::create_dir_all(dir.root())?;
        let db = Arc::new(StateDb::open(&dir.state())?);
        let cache = Arc::new(ObjectCache::open(dir.cache(), config.cache_budget)?);
        Ok(Shared {
            db,
            content: Content::new(cache.clone()),
            trees: TreeCache::new(cache),
            clients: Clients::new(config.clients.clone()),
            config,
        })
    }

    /// Run a blocking state-database call off the async runtime.
    pub async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&crate::state::StateDb) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || f(&db)).await?
    }
}
