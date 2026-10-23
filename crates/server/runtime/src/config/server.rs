//! The top-level configuration and its sections.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

use rill_lander::LandPolicy;
use rill_policy::RiskConfig;
use rill_review::votes::LabelSet;
use serde::{Deserialize, Serialize};

use super::{AuthConfig, IdentityConfig, MetadataConfig, StorageConfig};
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
    /// How the process stops.
    pub shutdown: ShutdownConfig,
    /// TLS for both listeners; plain text when absent.
    pub tls: Option<TlsConfig>,
    /// Logs and traces.
    pub telemetry: TelemetryConfig,
    /// Where objects and metadata live.
    pub storage: StorageConfig,
    /// Where metadata lives, when not beside the objects.
    pub metadata: MetadataConfig,
    /// Static bearer tokens (dev mode only).
    pub auth: AuthConfig,
    /// Token keys, lifetimes and instance administrators.
    pub identity: IdentityConfig,
    /// The audit log.
    pub audit: super::AuditConfig,
    /// Encryption of stored objects.
    pub encryption: super::EncryptionConfig,
    /// Structural merge of text conflicts.
    pub merge: rill_merge_syntax::MergeConfig,
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
    /// Per-principal request rate limits.
    pub rate_limit: super::RateLimitConfig,
    /// Remote execution.
    pub exec: super::ExecConfig,
    /// Notifications: email, insecure URLs, digest hour.
    pub notify: super::NotifyConfig,
    /// File holding the hex P-256 secret key the server signs commit
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
            shutdown: ShutdownConfig::default(),
            tls: None,
            telemetry: TelemetryConfig::default(),
            storage: StorageConfig::Memory,
            metadata: MetadataConfig::Storage,
            auth: AuthConfig::default(),
            identity: IdentityConfig::default(),
            audit: super::AuditConfig::default(),
            encryption: super::EncryptionConfig::default(),
            merge: rill_merge_syntax::MergeConfig::default(),
            lander: LandPolicy::default(),
            risk: RiskConfig::default(),
            labels: LabelSet::default(),
            verify: VerifyConfig::default(),
            postland: PostLandConfig::default(),
            rate_limit: super::RateLimitConfig::default(),
            exec: super::ExecConfig::default(),
            notify: super::NotifyConfig::default(),
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

/// How log lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Text,
    /// One JSON object per line, with the request's span fields.
    Json,
}

/// Logs and traces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub log_format: LogFormat,
    /// OTLP/HTTP endpoint spans are exported to; none when absent.
    pub otlp_endpoint: Option<String>,
    /// The `service.name` traces carry.
    pub service_name: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        TelemetryConfig {
            log_format: LogFormat::Text,
            otlp_endpoint: None,
            service_name: "rill".into(),
        }
    }
}

/// TLS for the gRPC and HTTP listeners, from PEM files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Certificate chain, leaf first.
    pub cert: std::path::PathBuf,
    /// Private key of the leaf certificate.
    pub key: std::path::PathBuf,
    /// When set, every connection must present a client certificate this
    /// CA signed (mutual TLS).
    pub client_ca: Option<std::path::PathBuf>,
}

/// How a process stops after it is told to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShutdownConfig {
    /// Seconds between turning unready and refusing requests, for load
    /// balancers to stop sending any.
    pub drain_secs: u64,
    /// Seconds in-flight requests and running jobs get to finish; jobs
    /// still running then are given back to their queues.
    pub grace_secs: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        ShutdownConfig {
            drain_secs: 5,
            grace_secs: 30,
        }
    }
}

/// Pre-land verification configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VerifyConfig {
    /// Run `.rill/verify.toml` through remote execution before landing.
    /// When false every landing verifies green.
    pub enabled: bool,
    /// Platform properties every step asks for, matching the workers that
    /// should run them; a step's own `platform` adds to these.
    pub platform: BTreeMap<String, String>,
    /// Environment every step gets beneath its own `env`. Workers start a
    /// command with only its action's environment, so this is where
    /// `PATH` comes from.
    pub env: BTreeMap<String, String>,
    /// Days a verification run, and so its logs and artifacts, stays
    /// readable.
    pub retention_days: u64,
    /// What a failure of a flaky test counts for.
    pub flakes: FlakeConfig,
}

/// How flaky failures are confirmed and chronic flakes quarantined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlakeConfig {
    /// A failure whose every target scores above this (per mille) is run
    /// again before it counts.
    pub rerun_above_permille: u32,
    /// Runs again at most.
    pub reruns: u32,
    /// A failing target scoring above this (per mille) with at least
    /// `quarantine_flips` flips is quarantined.
    pub quarantine_above_permille: u32,
    pub quarantine_flips: u32,
}

impl Default for FlakeConfig {
    fn default() -> Self {
        FlakeConfig {
            rerun_above_permille: 200,
            reruns: 2,
            quarantine_above_permille: 500,
            quarantine_flips: 5,
        }
    }
}

/// The `PATH` steps get unless configured otherwise.
pub const DEFAULT_STEP_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

impl Default for VerifyConfig {
    fn default() -> Self {
        VerifyConfig {
            enabled: false,
            platform: BTreeMap::new(),
            env: BTreeMap::from([("PATH".to_string(), DEFAULT_STEP_PATH.to_string())]),
            retention_days: 30,
            flakes: FlakeConfig::default(),
        }
    }
}

/// Post-land verification worker configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostLandConfig {
    /// Run the post-land verifier loop.
    pub enabled: bool,
    /// Seconds between sweeps.
    pub interval_secs: u64,
    /// Most landings verified together; batches grow toward it while
    /// trunk stays green and shrink after red.
    pub batch_size: u64,
    /// Seconds between runs of the whole plan on the head; between them
    /// batches run only the steps their landings touched.
    pub full_every_secs: u64,
}

impl Default for PostLandConfig {
    fn default() -> Self {
        PostLandConfig {
            enabled: false,
            interval_secs: 30,
            batch_size: rill_lander::postland::DEFAULT_BATCH_SIZE,
            full_every_secs: rill_lander::postland::DEFAULT_FULL_EVERY.as_secs(),
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
            [[auth.tokens]]
            token = "t1"
            principal_name = "alice"
            [verify]
            enabled = true
            [identity]
            admins = ["alice"]
            [identity.keys]
            backend = "vault"
            address = "https://vault:8200"
            key = "rill"
            [identity.lifetimes]
            agent_secs = 600
        "#;
        let config = ServerConfig::from_toml(text).unwrap();
        assert!(config.dev_mode);
        assert_eq!(config.listen_grpc.port(), 9000);
        assert!(matches!(config.storage, StorageConfig::Local { .. }));
        assert!(config.verify.enabled);
        assert_eq!(config.auth.tokens[0].principal_name, "alice");
        assert_eq!(config.auth.tokens[0].kind, PrincipalKind::Human);
        assert_eq!(config.identity.admins, ["alice"]);
        assert!(matches!(
            config.identity.keys,
            rill_identity::keys::KeyServiceConfig::Vault { ref mount, .. } if mount == "transit"
        ));
        assert_eq!(config.identity.lifetimes.agent_secs, 600);
        assert_eq!(config.identity.lifetimes.human_secs, 3600);
    }
}
