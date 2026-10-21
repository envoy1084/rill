//! The Rill server: the `rill.v1` gRPC API, the HTTP object protocol, a
//! per-repository event bus and the background workers (lander, post-land
//! verifier, search indexer).
//!
//! This crate is a library; the `rill-server` binary in `rill-cli` parses
//! flags, builds a [`ServerConfig`] and calls [`run`]. Tests and embedders
//! use [`build_state`] plus [`serve_with_state`] to bind ephemeral ports.
//!
//! Layout:
//!
//! * [`config`]: configuration file and environment overrides;
//! * [`state`]: shared state and lazily built per-repository contexts;
//! * [`auth`]: bearer-token authentication and the [`auth::Permit`] check;
//! * [`services`]: one module per gRPC service;
//! * [`objects`]: the HTTP object protocol and health endpoints;
//! * [`workers`]: background tasks;
//! * [`error`]: the error type and its gRPC status mapping.

pub mod auth;
pub mod config;
pub mod error;
pub mod objects;
mod serve;
pub mod services;
mod shutdown;
pub mod state;
pub mod telemetry;
mod tls;
pub mod workers;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

pub use config::{AuthConfig, Roles, ServerConfig, ShutdownConfig, StorageConfig, TlsConfig};
pub use error::{Error, Result};
pub use serve::{serve_with_state, Listeners};
pub use shutdown::serve_until;
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

/// Run the server on the configured addresses until `signal` fires, then
/// drain and stop within the configured deadline.
pub async fn run(config: ServerConfig, signal: CancellationToken) -> Result<()> {
    let listeners = Listeners::bind(config.listen_grpc, config.listen_http).await?;
    let state = build_state(config, CancellationToken::new()).await?;
    shutdown::serve_until(state, listeners, signal).await
}
