//! Process-wide server state shared by every handler.

use std::sync::Arc;

use rill_policy::StaticTokenAuthenticator;
use rill_store::Stores;
use tokio_util::sync::CancellationToken;

use super::registry::RepoRegistry;
use super::stores::open_stores;
use crate::auth;
use crate::config::ServerConfig;
use crate::error::{Error, Result};

/// Everything a request handler can reach.
pub struct AppState {
    /// Object and metadata stores.
    pub stores: Stores,
    /// The configuration the server started with.
    pub config: Arc<ServerConfig>,
    /// Static tokens (dev mode only).
    pub static_tokens: Option<StaticTokenAuthenticator>,
    /// Token keys and revocations as this server last read them.
    pub identity: Arc<crate::identity::IdentityView>,
    /// Per-repository service graphs.
    pub repos: RepoRegistry,
    /// Signs commit attestations.
    pub attestor: Arc<rill_policy::Signer>,
    /// Cancelled when the server is shutting down; workers watch it.
    pub shutdown: CancellationToken,
    /// Draining, stopping and aborting.
    pub lifecycle: Arc<super::Lifecycle>,
    /// Every metric the server keeps.
    pub metrics: Arc<crate::telemetry::Metrics>,
    /// Process start time, for health reporting.
    pub started_at: std::time::Instant,
}

impl AppState {
    /// Open the configured storage and build the state. Workers are
    /// spawned lazily per repository (or eagerly via [`RepoRegistry::warm`]).
    pub async fn new(config: ServerConfig, shutdown: CancellationToken) -> Result<Arc<Self>> {
        let stores = open_stores(&config.storage, &config.metadata).await?;
        Self::with_stores(config, stores, shutdown).await
    }

    /// Build the state over already-open stores.
    pub async fn with_stores(
        config: ServerConfig,
        stores: Stores,
        shutdown: CancellationToken,
    ) -> Result<Arc<Self>> {
        let config = Arc::new(config);
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let stores = stores.with_meta(Arc::new(crate::telemetry::InstrumentedMetaStore::new(
            stores.meta.clone(),
            metrics.clone(),
        )));
        let static_tokens = auth::static_tokens(&config)?;
        let identity = crate::identity::IdentityView::open(&stores, &config.identity).await?;
        let attestor = Arc::new(load_attestor(&config)?);
        let lifecycle = Arc::new(super::Lifecycle::new(shutdown.clone()));
        let repos = RepoRegistry::new(
            stores.clone(),
            config.clone(),
            attestor.clone(),
            lifecycle.clone(),
        );
        Ok(Arc::new(AppState {
            stores,
            config,
            static_tokens,
            identity,
            repos,
            attestor,
            shutdown,
            lifecycle,
            metrics,
            started_at: std::time::Instant::now(),
        }))
    }

    /// Whether anonymous requests are allowed and everything is permitted.
    pub fn dev_mode(&self) -> bool {
        self.config.dev_mode
    }
}

/// The key commit attestations are signed with.
fn load_attestor(config: &ServerConfig) -> Result<rill_policy::Signer> {
    match &config.attestation_key {
        Some(path) => {
            let contents = std::fs::read_to_string(path)
                .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
            rill_policy::KeyFile::decode(contents.trim())
                .map_err(|e| Error::Config(format!("{}: {e}", path.display())))
        }
        None => {
            if !config.dev_mode {
                tracing::warn!(
                    "no attestation_key configured; commit attestations use a per-process key"
                );
            }
            Ok(rill_policy::Signer::generate())
        }
    }
}
