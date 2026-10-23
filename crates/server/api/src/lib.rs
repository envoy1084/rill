//! The Rill API: the `rill.v1` gRPC services and the HTTP object protocol,
//! served over the state `rill-runtime` builds.
//!
//! The `rill` binary in `rill-cli` parses flags, builds a [`ServerConfig`]
//! and calls [`run`]. Tests and embedders use [`build_state`] plus
//! [`serve_with_state`] to bind ephemeral ports.
//!
//! * [`auth`]: bearer-token authentication and the [`auth::Permit`] check;
//! * [`services`]: one module per gRPC service;
//! * [`objects`]: the HTTP object protocol and health endpoints;
//! * [`grants`], [`scim`], [`signin`]: read grants, SCIM provisioning, sign-in.

pub mod auth;
pub mod grants;
pub mod listing;
pub mod objects;
mod rpc_codes;
pub mod scim;
mod serve;
pub mod services;
mod shutdown;
pub mod signin;
mod tls;

use tokio_util::sync::CancellationToken;

pub use rill_runtime::{
    build_state, AppState, AuthConfig, Error, RepoContext, Result, Roles, ServerConfig,
    ShutdownConfig, StaticToken, StorageConfig, TlsConfig,
};
pub use serve::{serve_with_state, Listeners};
pub use shutdown::serve_until;

/// Run the server on the configured addresses until `signal` fires, then
/// drain and stop within the configured deadline.
pub async fn run(config: ServerConfig, signal: CancellationToken) -> Result<()> {
    let listeners = Listeners::bind(config.listen_grpc, config.listen_http).await?;
    let state = build_state(config, CancellationToken::new()).await?;
    shutdown::serve_until(state, listeners, signal).await
}
