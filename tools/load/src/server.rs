//! An embedded dev server, when no endpoint is given.

use std::net::SocketAddr;

use rill_api::{Listeners, ServerConfig, StorageConfig};
use tokio_util::sync::CancellationToken;

/// A dev-mode server in this process; stops when dropped.
pub struct EmbeddedServer {
    pub endpoint: String,
    shutdown: CancellationToken,
}

impl EmbeddedServer {
    /// Start a memory-backed dev server on a free port.
    pub async fn start() -> anyhow::Result<Self> {
        let any: SocketAddr = "127.0.0.1:0".parse()?;
        let listeners = Listeners::bind(any, any).await?;
        let (grpc, _) = listeners.addrs()?;
        let config = ServerConfig {
            dev_mode: true,
            storage: StorageConfig::Memory,
            ..ServerConfig::default()
        };
        let shutdown = CancellationToken::new();
        let state = rill_api::build_state(config, shutdown.clone()).await?;
        let token = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = rill_api::serve_with_state(state, listeners, token).await {
                eprintln!("embedded server stopped: {e}");
            }
        });
        Ok(EmbeddedServer {
            endpoint: format!("http://{grpc}"),
            shutdown,
        })
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
