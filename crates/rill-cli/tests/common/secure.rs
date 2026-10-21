//! A server with authentication on, on a local store, minting tokens with
//! its own key.

use std::net::SocketAddr;
use std::sync::Arc;

use rill_api::config::IdentityConfig;
use rill_api::{AppState, Listeners, ServerConfig, StorageConfig};
use rill_identity::keys::KeyServiceConfig;
use rill_sdk::{Client, ClientConfig};
use rill_types::Principal;
use tokio_util::sync::CancellationToken;

pub struct SecureServer {
    pub grpc: SocketAddr,
    pub http: SocketAddr,
    pub state: Arc<AppState>,
    shutdown: CancellationToken,
    _dir: tempfile::TempDir,
}

impl SecureServer {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            storage: StorageConfig::Local {
                dir: dir.path().join("data"),
            },
            identity: IdentityConfig {
                keys: KeyServiceConfig::Local {
                    master_key_hex: Some("17".repeat(32)),
                    master_key_file: None,
                },
                ..IdentityConfig::default()
            },
            ..ServerConfig::default()
        };
        let any: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listeners = Listeners::bind(any, any).await.unwrap();
        let (grpc, http) = listeners.addrs().unwrap();
        let shutdown = CancellationToken::new();
        let state = rill_api::build_state(config, shutdown.clone())
            .await
            .unwrap();
        let (serving, token) = (state.clone(), shutdown.clone());
        tokio::spawn(async move {
            rill_api::serve_with_state(serving, listeners, token)
                .await
                .unwrap();
        });
        SecureServer {
            grpc,
            http,
            state,
            shutdown,
            _dir: dir,
        }
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}", self.grpc)
    }

    /// A token for `who` signed by the server.
    pub fn token(&self, who: &Principal) -> String {
        let issuer = self.state.identity.issuer().unwrap();
        let ttl = issuer.lifetimes().max_for(who.kind);
        let claims = rill_policy::TokenClaims::new("default", rill_types::Timestamp::now(), ttl);
        issuer.issue(who, &claims, None).unwrap()
    }

    pub async fn client(&self, who: &Principal) -> Client {
        let mut config = ClientConfig::new(self.endpoint());
        config.token = Some(self.token(who));
        Client::connect(config).await.unwrap()
    }

    pub fn stop(&self) {
        self.shutdown.cancel();
    }
}
