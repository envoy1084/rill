//! Client configuration: endpoint, credentials and connection tuning.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::error::{Error, Result};

/// Environment variable naming the server endpoint.
pub const ENDPOINT_ENV: &str = "RILL_ENDPOINT";
/// Environment variable carrying the bearer token.
pub const TOKEN_ENV: &str = "RILL_TOKEN";

/// How to reach a Rill server.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// `http://host:port` or `https://host:port`.
    pub endpoint: String,
    /// Bearer token sent as `authorization` metadata on every request.
    pub token: Option<String>,
    /// Per-request timeout; `None` waits indefinitely (streams always do).
    pub timeout: Option<Duration>,
    /// `user-agent` header value.
    pub user_agent: String,
}

impl ClientConfig {
    /// Configuration for `endpoint` with no token and default tuning.
    pub fn new(endpoint: impl Into<String>) -> Self {
        ClientConfig {
            endpoint: endpoint.into(),
            token: None,
            timeout: Some(Duration::from_secs(30)),
            user_agent: default_user_agent(),
        }
    }

    /// Set the bearer token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set the per-request timeout.
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read `RILL_ENDPOINT` and `RILL_TOKEN`.
    pub fn from_env() -> Result<Self> {
        let endpoint = std::env::var(ENDPOINT_ENV)
            .map_err(|_| Error::config(format!("{ENDPOINT_ENV} is not set")))?;
        let token = std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty());
        Ok(ClientConfig {
            token,
            ..ClientConfig::new(endpoint)
        })
    }

    /// Load `[server] endpoint, token` from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::config(format!("reading {}: {e}", path.display())))?;
        Self::parse(&text).map_err(|e| Error::config(format!("{}: {e}", path.display())))
    }

    /// Parse the config file format from a string.
    pub fn parse(text: &str) -> Result<Self> {
        let file: ConfigFile =
            toml::from_str(text).map_err(|e| Error::config(format!("invalid config: {e}")))?;
        let server = file
            .server
            .ok_or_else(|| Error::config("missing [server] section"))?;
        Ok(ClientConfig {
            token: server.token.filter(|t| !t.is_empty()),
            timeout: server
                .timeout_secs
                .map(Duration::from_secs)
                .or(Some(Duration::from_secs(30))),
            ..ClientConfig::new(server.endpoint)
        })
    }

    /// `~/.config/rill/config.toml`, if the platform has a config directory.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rill").join("config.toml"))
    }

    /// Environment first, then the default config file.
    pub fn discover() -> Result<Self> {
        if let Ok(cfg) = Self::from_env() {
            return Ok(cfg);
        }
        match Self::default_path() {
            Some(path) if path.exists() => Self::load(path),
            _ => Err(Error::config(format!(
                "no {ENDPOINT_ENV} in the environment and no config file"
            ))),
        }
    }
}

fn default_user_agent() -> String {
    format!("rill-sdk/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    server: Option<ServerSection>,
}

#[derive(Debug, Deserialize)]
struct ServerSection {
    endpoint: String,
    token: Option<String>,
    timeout_secs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_section() {
        let cfg = ClientConfig::parse(
            "[server]\nendpoint = \"http://localhost:7000\"\ntoken = \"abc\"\n",
        )
        .unwrap();
        assert_eq!(cfg.endpoint, "http://localhost:7000");
        assert_eq!(cfg.token.as_deref(), Some("abc"));
        assert_eq!(cfg.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn missing_section_is_config_error() {
        let err = ClientConfig::parse("[other]\nx = 1\n").unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }
}
