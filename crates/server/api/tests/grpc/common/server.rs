//! An in-process server on ephemeral ports.

use std::sync::Arc;

use rill_api::{build_state, serve_with_state, AppState, Listeners, ServerConfig};
use tokio_util::sync::CancellationToken;

/// A running server and how to reach it.
pub(crate) struct TestServer {
    pub(crate) grpc: String,
    pub(crate) http: std::net::SocketAddr,
    shutdown: CancellationToken,
    pub(crate) state: Arc<AppState>,
    task: tokio::task::JoinHandle<rill_api::Result<()>>,
}

impl TestServer {
    pub(crate) async fn start(config: ServerConfig) -> Self {
        let shutdown = CancellationToken::new();
        let state = build_state(config, shutdown.clone()).await.unwrap();
        Self::serve(state, shutdown).await
    }

    /// A server over `stores`, which other test servers may share.
    pub(crate) async fn start_on(config: ServerConfig, stores: rill_store::Stores) -> Self {
        let shutdown = CancellationToken::new();
        let state = AppState::with_stores(config, stores, shutdown.clone())
            .await
            .unwrap();
        state.repos.warm().await.unwrap();
        Self::serve(state, shutdown).await
    }

    async fn serve(state: Arc<AppState>, shutdown: CancellationToken) -> Self {
        let listeners = Listeners::bind(
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (grpc_addr, http) = listeners.addrs().unwrap();
        let task = tokio::spawn(serve_with_state(state.clone(), listeners, shutdown.clone()));
        TestServer {
            grpc: format!("http://{grpc_addr}"),
            http,
            shutdown,
            state,
            task,
        }
    }

    /// A token the server signed for `principal`, as a sign-in would issue.
    pub(crate) fn token_for(&self, principal: &rill_types::Principal) -> String {
        let issuer = self.state.identity.issuer().unwrap();
        let ttl = issuer.lifetimes().max_for(principal.kind);
        let claims = rill_policy::TokenClaims::new("default", rill_types::Timestamp::now(), ttl);
        issuer.issue(principal, &claims, None).unwrap()
    }

    /// A token for `principal` narrowed to `capability`.
    pub(crate) fn token_with(
        &self,
        principal: &rill_types::Principal,
        capability: &rill_policy::Capability,
    ) -> String {
        let issuer = self.state.identity.issuer().unwrap();
        let ttl = issuer.lifetimes().max_for(principal.kind);
        let claims = rill_policy::TokenClaims::new("default", rill_types::Timestamp::now(), ttl);
        issuer.issue(principal, &claims, Some(capability)).unwrap()
    }

    pub(crate) async fn stop(self) {
        self.shutdown.cancel();
        self.task.await.unwrap().unwrap();
    }
}

pub(crate) fn dev_config() -> ServerConfig {
    ServerConfig {
        dev_mode: true,
        ..ServerConfig::default()
    }
}
