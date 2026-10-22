//! An in-process dev server for tests of code built on the SDK: dev mode
//! (no authentication), memory storage, a free local port.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig, StorageConfig};
use tokio_util::sync::CancellationToken;

use crate::client::Client;
use crate::config::ClientConfig;
use crate::error::{Error, Result};

/// A dev server in this process; stops when dropped.
pub struct DevServer {
    endpoint: String,
    shutdown: CancellationToken,
}

impl DevServer {
    /// The dev-mode, memory-backed configuration [`DevServer::start`]
    /// uses: no rate limit, since a test is one principal doing everyone's
    /// work.
    pub fn config() -> ServerConfig {
        let mut config = ServerConfig {
            dev_mode: true,
            storage: StorageConfig::Memory,
            ..ServerConfig::default()
        };
        config.rate_limit.per_principal_per_sec = 0;
        config
    }

    /// Start a server with [`DevServer::config`].
    pub async fn start() -> Result<Self> {
        Self::start_with(Self::config()).await
    }

    /// Start a server with `config` on a free local port.
    pub async fn start_with(config: ServerConfig) -> Result<Self> {
        let failed = |e: &dyn std::fmt::Display| Error::config(format!("dev server: {e}"));
        let any: SocketAddr = "127.0.0.1:0".parse().map_err(|e| failed(&e))?;
        let listeners = Listeners::bind(any, any).await.map_err(|e| failed(&e))?;
        let (api, _) = listeners.addrs().map_err(|e| failed(&e))?;
        let shutdown = CancellationToken::new();
        let state = rill_api::build_state(config, shutdown.clone())
            .await
            .map_err(|e| failed(&e))?;
        let token = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = rill_api::serve_with_state(state, listeners, token).await {
                tracing::error!(error = %e, "dev server stopped");
            }
        });
        Ok(DevServer {
            endpoint: format!("http://{api}"),
            shutdown,
        })
    }

    /// The API endpoint, `http://127.0.0.1:<port>`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// A client connected to the server.
    pub async fn client(&self) -> Result<Client> {
        Client::connect(ClientConfig::new(self.endpoint.clone())).await
    }
}

impl Drop for DevServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
