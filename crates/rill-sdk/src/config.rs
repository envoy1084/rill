//! Client configuration: endpoint, credentials and connection tuning.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::error::{Error, Result};

/// Environment variable naming the server endpoint.
pub const ENDPOINT_ENV: &str = "RILL_ENDPOINT";
/// Environment variable carrying the bearer token.
pub const TOKEN_ENV: &str = "RILL_TOKEN";
/// Environment variable naming a PEM file of CAs to trust beyond the
/// system's.
pub const CA_FILE_ENV: &str = "RILL_CA_FILE";
/// Environment variable naming a PEM client certificate, for servers that
/// require one.
pub const CLIENT_CERT_ENV: &str = "RILL_CLIENT_CERT";
/// Environment variable naming the client certificate's PEM key.
pub const CLIENT_KEY_ENV: &str = "RILL_CLIENT_KEY";

/// TLS settings for `https://` endpoints. The server's certificate is
/// always checked, against the system's roots and `ca_file`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsOptions {
    /// Extra CAs to trust (PEM).
    pub ca_file: Option<PathBuf>,
    /// Client certificate (PEM), for mutual TLS.
    pub client_cert: Option<PathBuf>,
    /// The client certificate's key (PEM).
    pub client_key: Option<PathBuf>,
}

impl TlsOptions {
    /// Read `RILL_CA_FILE`, `RILL_CLIENT_CERT` and `RILL_CLIENT_KEY`.
    pub fn from_env() -> Self {
        let path = |name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        TlsOptions {
            ca_file: path(CA_FILE_ENV),
            client_cert: path(CLIENT_CERT_ENV),
            client_key: path(CLIENT_KEY_ENV),
        }
    }
}

/// A stored sign-in to use when no token is given.
#[derive(Clone)]
pub struct SessionConfig {
    /// The session to use.
    pub credentials: crate::auth::Credentials,
    /// Where refreshed credentials are saved.
    pub store: Option<std::sync::Arc<dyn crate::auth::CredentialStore>>,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

/// How to reach a Rill server.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// `http://host:port` or `https://host:port`.
    pub endpoint: String,
    /// Bearer token sent as `authorization` metadata on every request.
    pub token: Option<String>,
    /// A signed-in session, used when `token` is unset; refreshed in the
    /// background.
    pub session: Option<SessionConfig>,
    /// A workload credential exchanged for an agent token, used when
    /// neither `token` nor `session` is set. Needs [`crate::Client::connect`].
    pub workload: Option<crate::auth::WorkloadConfig>,
    /// Per-request timeout; `None` waits indefinitely (streams always do).
    pub timeout: Option<Duration>,
    /// `user-agent` header value.
    pub user_agent: String,
    /// TLS settings for `https://` endpoints.
    pub tls: TlsOptions,
}

impl ClientConfig {
    /// Configuration for `endpoint` with no token and default tuning. TLS
    /// files come from `RILL_CA_FILE`, `RILL_CLIENT_CERT` and
    /// `RILL_CLIENT_KEY` when set.
    pub fn new(endpoint: impl Into<String>) -> Self {
        ClientConfig {
            endpoint: endpoint.into(),
            token: None,
            session: None,
            workload: None,
            timeout: Some(Duration::from_secs(30)),
            user_agent: default_user_agent(),
            tls: TlsOptions::from_env(),
        }
    }

    /// Set the TLS settings.
    pub fn with_tls(mut self, tls: TlsOptions) -> Self {
        self.tls = tls;
        self
    }

    /// Set the bearer token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Use a stored session, saving refreshes to `store`.
    pub fn with_session(
        mut self,
        credentials: crate::auth::Credentials,
        store: Option<std::sync::Arc<dyn crate::auth::CredentialStore>>,
    ) -> Self {
        self.session = Some(SessionConfig { credentials, store });
        self
    }

    /// Set the per-request timeout.
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read `RILL_ENDPOINT`, `RILL_TOKEN` and, for agents, the workload
    /// variables (`RILL_WORKLOAD_TOKEN_FILE`, `RILL_ORG`, `RILL_AGENT`,
    /// `RILL_MODEL`).
    pub fn from_env() -> Result<Self> {
        let endpoint = std::env::var(ENDPOINT_ENV)
            .map_err(|_| Error::config(format!("{ENDPOINT_ENV} is not set")))?;
        let token = std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty());
        Ok(ClientConfig {
            token,
            workload: crate::auth::WorkloadConfig::from_env(),
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
        let base = ClientConfig::new(server.endpoint);
        let tls = TlsOptions {
            ca_file: server.ca_file.or(base.tls.ca_file.clone()),
            client_cert: server.client_cert.or(base.tls.client_cert.clone()),
            client_key: server.client_key.or(base.tls.client_key.clone()),
        };
        Ok(ClientConfig {
            token: server.token.filter(|t| !t.is_empty()),
            timeout: server
                .timeout_secs
                .map(Duration::from_secs)
                .or(Some(Duration::from_secs(30))),
            tls,
            ..base
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
    ca_file: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
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
