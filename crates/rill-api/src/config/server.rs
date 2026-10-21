//! The top-level configuration and its sections.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

use rill_lander::LandPolicy;
use rill_policy::RiskConfig;
use rill_review::votes::LabelSet;
use serde::{Deserialize, Serialize};

use super::{AuthConfig, MetadataConfig, StorageConfig};
use crate::error::{Error, Result};

/// Default gRPC listen address.
pub const DEFAULT_GRPC_ADDR: &str = "127.0.0.1:7420";
/// Default HTTP (object protocol and health) listen address.
pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7421";

/// Top-level server configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Address the gRPC API listens on.
    pub listen_grpc: SocketAddr,
    /// Address the object protocol and health endpoints listen on.
    pub listen_http: SocketAddr,
    /// Which parts of the service this process runs.
    pub roles: Roles,
    /// Where objects and metadata live.
    pub storage: StorageConfig,
    /// Where metadata lives, when not beside the objects.
    pub metadata: MetadataConfig,
    /// How bearer tokens are authenticated.
    pub auth: AuthConfig,
    /// Landing pipeline policy (verification modes, retries, auto-revert).
    pub lander: LandPolicy,
    /// Risk classification rules applied to every change under review.
    pub risk: RiskConfig,
    /// Vote labels available on every repository.
    pub labels: LabelSet,
    /// Pre-land verification.
    pub verify: VerifyConfig,
    /// Post-land verification worker.
    pub postland: PostLandConfig,
    /// File holding the hex Ed25519 seed the server signs commit
    /// attestations with. Without it a key is generated per process, so
    /// attestations do not verify across restarts.
    pub attestation_key: Option<std::path::PathBuf>,
    /// When true, requests without a bearer token act as the `dev` human
    /// principal and every permission check passes. Never enable this on a
    /// server reachable by anyone you do not trust.
    pub dev_mode: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            listen_grpc: DEFAULT_GRPC_ADDR
                .parse()
                .expect("default gRPC address is valid"),
            listen_http: DEFAULT_HTTP_ADDR
                .parse()
                .expect("default HTTP address is valid"),
            roles: Roles::default(),
            storage: StorageConfig::Memory,
            metadata: MetadataConfig::Storage,
            auth: AuthConfig::default(),
            lander: LandPolicy::default(),
            risk: RiskConfig::default(),
            labels: LabelSet::default(),
            verify: VerifyConfig::default(),
            postland: PostLandConfig::default(),
            attestation_key: None,
            dev_mode: false,
        }
    }
}

/// Which parts of the service a process runs. Any number of processes may
/// run either role: API servers are stateless, and background work is
/// shared through leases and job claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Roles {
    /// Serve the gRPC API and the object protocol.
    pub api: bool,
    /// Run landers, post-land verification, job workers and retention.
    pub workers: bool,
}

impl Default for Roles {
    fn default() -> Self {
        Roles {
            api: true,
            workers: true,
        }
    }
}

impl Roles {
    /// Parse `api`, `workers` or `api,workers`.
    pub fn parse(spec: &str) -> Result<Self> {
        let mut roles = Roles {
            api: false,
            workers: false,
        };
        for role in spec.split(',').map(str::trim).filter(|r| !r.is_empty()) {
            match role {
                "api" => roles.api = true,
                "workers" => roles.workers = true,
                other => {
                    return Err(Error::Config(format!(
                        "unknown role {other:?}; expected api or workers"
                    )))
                }
            }
        }
        if !roles.api && !roles.workers {
            return Err(Error::Config("at least one role is needed".into()));
        }
        Ok(roles)
    }
}

/// Sandbox backend for pre-land verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxKind {
    /// Run commands directly on the host in a temporary directory.
    #[default]
    Local,
}

/// Pre-land verification configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VerifyConfig {
    /// Run `.rill/verify.toml` before landing. When false every landing
    /// verifies green.
    pub enabled: bool,
    /// Which sandbox executes verification steps.
    pub sandbox: SandboxKind,
    /// Whether sandboxed commands may use the network (advisory for the
    /// local sandbox).
    pub network: bool,
}

/// Post-land verification worker configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostLandConfig {
    /// Run the post-land verifier loop.
    pub enabled: bool,
    /// Seconds between sweeps.
    pub interval_secs: u64,
    /// Landings verified per sweep when the worker is behind.
    pub batch_size: u64,
}

impl Default for PostLandConfig {
    fn default() -> Self {
        PostLandConfig {
            enabled: false,
            interval_secs: 30,
            batch_size: rill_lander::postland::DEFAULT_BATCH_SIZE,
        }
    }
}

impl ServerConfig {
    /// Parse a TOML document.
    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| Error::Config(e.to_string()))
    }

    /// Serialize to TOML.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))
    }

    /// Load from `path` (if given) and then apply `RILL_*` environment
    /// overrides from the process environment.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut config = match path {
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .map_err(|e| Error::Config(format!("reading {}: {e}", p.display())))?;
                Self::from_toml(&text)?
            }
            None => Self::default(),
        };
        let env: BTreeMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("RILL_"))
            .collect();
        config.apply_env(|k| env.get(k).cloned())?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use rill_types::PrincipalKind;

    use super::*;

    #[test]
    fn default_roundtrips_through_toml() {
        let config = ServerConfig::default();
        let text = config.to_toml().unwrap();
        assert_eq!(ServerConfig::from_toml(&text).unwrap(), config);
    }

    #[test]
    fn parses_partial_file() {
        let text = r#"
            dev_mode = true
            listen_grpc = "0.0.0.0:9000"
            [storage]
            kind = "local"
            dir = "/var/lib/rill"
            [auth]
            kind = "static"
            [[auth.tokens]]
            token = "t1"
            principal_name = "alice"
            [verify]
            enabled = true
        "#;
        let config = ServerConfig::from_toml(text).unwrap();
        assert!(config.dev_mode);
        assert_eq!(config.listen_grpc.port(), 9000);
        assert!(matches!(config.storage, StorageConfig::Local { .. }));
        assert!(config.verify.enabled);
        match config.auth {
            AuthConfig::Static { tokens } => {
                assert_eq!(tokens[0].principal_name, "alice");
                assert_eq!(tokens[0].kind, PrincipalKind::Human);
            }
            _ => panic!("expected static auth"),
        }
    }
}
