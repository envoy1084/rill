//! The state of a Rill server process: configuration, stores, the
//! per-repository contexts and their background workers (lander, post-land
//! verifier, search indexer), telemetry, the identity view and the audit
//! log. `rill-api` serves the gRPC API and the object protocol over it.
//!
//! * [`config`]: configuration file and environment overrides;
//! * [`state`]: shared state and lazily built per-repository contexts;
//! * [`workers`]: background tasks;
//! * [`fleet`]: the reviewer fleet's queues;
//! * [`notify`]: webhooks and personal notifications;
//! * [`identity`]: token keys, revocations and callers as this server sees them;
//! * [`audit`]: the audit log's projection, seals and denials;
//! * [`secrets`], [`quarantine`], [`writes`]: checks every stored version passes;
//! * [`rate`]: per-principal request rate limits;
//! * [`error`]: the server's error type and its gRPC status mapping.

pub mod audit;
pub mod config;
pub mod error;
pub mod exec;
pub mod fleet;
pub mod identity;
pub mod notify;
pub mod quarantine;
pub mod rate;
pub mod secrets;
pub mod state;
pub mod telemetry;
pub mod workers;
pub mod writes;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

pub use config::{
    AuthConfig, Roles, ServerConfig, ShutdownConfig, StaticToken, StorageConfig, TlsConfig,
};
pub use error::{Error, Result};
pub use state::{AppState, RepoContext};

/// Open storage and build the shared state without binding any sockets.
/// Workers for existing repositories start immediately.
pub async fn build_state(
    config: ServerConfig,
    shutdown: CancellationToken,
) -> Result<Arc<AppState>> {
    let state = AppState::new(config, shutdown).await?;
    let warmed = state.repos.warm().await?;
    tracing::info!(repositories = warmed, "state built");
    Ok(state)
}
