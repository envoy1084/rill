//! Server configuration: endpoint, token and default repository from
//! flags or the `RILL_ENDPOINT`, `RILL_TOKEN` and `RILL_REPO` variables.

use rill_sdk::{Client, ClientConfig};

use crate::error::{Error, Result};

/// Environment variable naming the default repository.
pub const REPO_ENV: &str = "RILL_REPO";

/// How the MCP server reaches Rill.
#[derive(Debug, Clone, Default)]
pub struct McpConfig {
    /// Server endpoint; falls back to `RILL_ENDPOINT` or the SDK config file.
    pub endpoint: Option<String>,
    /// Bearer token; falls back to `RILL_TOKEN`.
    pub token: Option<String>,
    /// Repository used when a tool call names none; falls back to `RILL_REPO`.
    pub repo: Option<String>,
}

impl McpConfig {
    /// Fill unset fields from the environment and the SDK's config
    /// discovery, producing the client configuration.
    pub fn resolve(&self) -> Result<(ClientConfig, Option<String>)> {
        let mut config = match &self.endpoint {
            Some(endpoint) => ClientConfig::new(endpoint.clone()),
            None => ClientConfig::discover().map_err(|e| Error::Config(e.to_string()))?,
        };
        if let Some(token) = &self.token {
            config.token = Some(token.clone());
        } else if config.token.is_none() {
            config.token = std::env::var(rill_sdk::config::TOKEN_ENV)
                .ok()
                .filter(|t| !t.is_empty());
        }
        let repo = self
            .repo
            .clone()
            .or_else(|| std::env::var(REPO_ENV).ok().filter(|r| !r.is_empty()));
        Ok((config, repo))
    }

    /// A lazily connecting client and the default repository. The
    /// connection is deferred so `initialize` and `tools/list` work even
    /// when the server is not yet reachable.
    pub fn connect_lazy(&self) -> Result<(Client, Option<String>)> {
        let (config, repo) = self.resolve()?;
        let client = Client::connect_lazy(config)?;
        Ok((client, repo))
    }
}
