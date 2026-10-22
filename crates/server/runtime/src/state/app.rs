//! Process-wide server state shared by every handler.

use std::sync::Arc;

use rill_policy::StaticTokenAuthenticator;
use rill_store::Stores;
use rill_types::{Principal, PrincipalKind};
use tokio_util::sync::CancellationToken;

use super::registry::RepoRegistry;
use super::stores::open_stores;
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
    /// Each org's data keys, when objects are sealed at rest.
    pub data_keys: Option<Arc<rill_identity::keys::DataKeyManager>>,
    /// Per-principal request rate limits, when on.
    pub rate_limits: Option<crate::rate::RateLimits>,
    /// Every metric the server keeps.
    pub metrics: Arc<crate::telemetry::Metrics>,
    /// Process start time, for health reporting.
    pub started_at: std::time::Instant,
}

impl AppState {
    /// Open the configured storage and build the state. Workers are
    /// spawned lazily per repository (or eagerly via [`RepoRegistry::warm`]).
    pub async fn new(config: ServerConfig, shutdown: CancellationToken) -> Result<Arc<Self>> {
        let sealer = (config.encryption.enabled(config.dev_mode)
            && !matches!(config.storage, crate::config::StorageConfig::Memory))
        .then(|| Arc::new(rill_store::OrgSealer::new()));
        let stores = open_stores(
            &config.storage,
            &config.metadata,
            sealer.clone().map(|s| s as Arc<dyn rill_store::Sealer>),
        )
        .await?;
        Self::build(config, stores, sealer, shutdown).await
    }

    /// Build the state over already-open stores.
    pub async fn with_stores(
        config: ServerConfig,
        stores: Stores,
        shutdown: CancellationToken,
    ) -> Result<Arc<Self>> {
        Self::build(config, stores, None, shutdown).await
    }

    async fn build(
        config: ServerConfig,
        stores: Stores,
        sealer: Option<Arc<rill_store::OrgSealer>>,
        shutdown: CancellationToken,
    ) -> Result<Arc<Self>> {
        let config = Arc::new(config);
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let stores = stores.with_meta(Arc::new(crate::telemetry::InstrumentedMetaStore::new(
            stores.meta.clone(),
            metrics.clone(),
        )));
        let static_tokens = static_tokens(&config)?;
        let identity = crate::identity::IdentityView::open(&stores, &config.identity).await?;
        // Compiling the secret rules takes a moment; do it before the
        // first commit needs them.
        tokio::task::spawn_blocking(|| {
            std::sync::LazyLock::force(&rill_policy::secrets::CATALOG);
        });
        let data_keys = sealer.map(|sealer| {
            let keys = Arc::new(rill_identity::keys::DataKeyManager::new(
                stores.identity(),
                identity.keys().key_service().clone(),
                crate::identity::system_actor(),
            ));
            sealer.set_source(keys.clone());
            tracing::info!("objects are encrypted at rest under per-org data keys");
            keys
        });
        let attestor = Arc::new(load_attestor(&config)?);
        let lifecycle = Arc::new(super::Lifecycle::new(shutdown.clone()));
        let repos = RepoRegistry::new(
            stores.clone(),
            config.clone(),
            attestor.clone(),
            identity.clone(),
            lifecycle.clone(),
        );
        let rate_limits = crate::rate::RateLimits::new(&config.rate_limit);
        Ok(Arc::new(AppState {
            stores,
            config,
            static_tokens,
            identity,
            repos,
            attestor,
            shutdown,
            lifecycle,
            data_keys,
            rate_limits,
            metrics,
            started_at: std::time::Instant::now(),
        }))
    }

    /// Whether anonymous requests are allowed and everything is permitted.
    pub fn dev_mode(&self) -> bool {
        self.config.dev_mode
    }

    /// Count a request from `ctx` against its principal's rate limit.
    pub fn admit(&self, ctx: &rill_policy::AuthContext) -> Result<()> {
        match &self.rate_limits {
            Some(limits) => limits.check(&ctx.principal.name),
            None => Ok(()),
        }
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

/// The static token table, when the configuration has one. Static tokens
/// are refused outside dev mode.
fn static_tokens(config: &ServerConfig) -> Result<Option<StaticTokenAuthenticator>> {
    if config.auth.tokens.is_empty() {
        return Ok(None);
    }
    if !config.dev_mode {
        return Err(Error::Config(
            "static tokens are only accepted in dev mode; issue tokens instead".into(),
        ));
    }
    let mut table = StaticTokenAuthenticator::new();
    for t in &config.auth.tokens {
        let principal = match t.kind {
            PrincipalKind::Human => Principal::human(&t.principal_name),
            PrincipalKind::Agent => Principal::agent(&t.principal_name),
            PrincipalKind::Service => Principal::service(&t.principal_name),
        };
        table.insert(t.token.clone(), principal, t.teams.clone());
    }
    Ok(Some(table))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StaticToken;

    #[test]
    fn static_tokens_need_dev_mode() {
        let mut config = ServerConfig::default();
        config.auth.tokens.push(StaticToken {
            token: "t".into(),
            principal_name: "alice".into(),
            kind: PrincipalKind::Human,
            teams: vec![],
        });
        assert!(matches!(static_tokens(&config), Err(Error::Config(_))));
        config.dev_mode = true;
        assert!(static_tokens(&config).unwrap().is_some());
    }
}
