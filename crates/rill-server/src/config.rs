//! Server configuration: a TOML file with `RILL_*` environment overrides.
//!
//! The file is optional; every section has a default that gives a usable
//! single-node development server (in-memory storage, dev mode, no
//! verification). Environment variables override individual fields so a
//! container can be configured without a file; see [`ServerConfig::apply_env`]
//! for the recognised names.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use rill_lander::LandPolicy;
use rill_policy::RiskConfig;
use rill_review::votes::LabelSet;
use rill_types::PrincipalKind;
use serde::{Deserialize, Serialize};

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
    /// Where objects and metadata live.
    pub storage: StorageConfig,
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
            storage: StorageConfig::Memory,
            auth: AuthConfig::default(),
            lander: LandPolicy::default(),
            risk: RiskConfig::default(),
            labels: LabelSet::default(),
            verify: VerifyConfig::default(),
            postland: PostLandConfig::default(),
            dev_mode: false,
        }
    }
}

/// Storage backend selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Everything in process memory; lost on exit. For tests and demos.
    Memory,
    /// Objects as files and metadata in redb under `dir`.
    Local {
        /// Data directory; created if missing.
        dir: PathBuf,
    },
    /// Objects in an S3-compatible bucket, metadata in redb under `meta_dir`.
    S3 {
        /// Bucket URL understood by `object_store`, e.g. `s3://bucket`.
        url: String,
        /// Key prefix inside the bucket.
        #[serde(default)]
        prefix: String,
        /// Directory holding the redb metadata store.
        meta_dir: PathBuf,
    },
}

/// One statically configured bearer token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticToken {
    /// The bearer token value.
    pub token: String,
    /// Name of the principal the token authenticates as.
    pub principal_name: String,
    /// Kind of that principal.
    #[serde(default = "default_kind")]
    pub kind: PrincipalKind,
    /// Teams the principal belongs to, for team-scoped relations.
    #[serde(default)]
    pub teams: Vec<String>,
}

fn default_kind() -> PrincipalKind {
    PrincipalKind::Human
}

/// Authentication configuration.
///
/// Both variants also accept Biscuit capability tokens minted by this
/// server's own issuer (see `IssueToken`), so agents can be handed narrow
/// tokens regardless of how humans sign in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthConfig {
    /// A fixed list of tokens.
    Static {
        /// The tokens.
        #[serde(default)]
        tokens: Vec<StaticToken>,
    },
    /// Biscuit tokens signed by a root key.
    Biscuit {
        /// Hex-encoded Ed25519 public key that verifies tokens.
        root_public_key_hex: String,
        /// Hex-encoded private key, when this server also issues tokens.
        /// Without it, `IssueToken` uses a key generated at startup.
        #[serde(default)]
        root_private_key_hex: Option<String>,
    },
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::Static { tokens: Vec::new() }
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

    /// Apply overrides from `lookup`, which answers environment variable
    /// names. Recognised variables:
    ///
    /// * `RILL_LISTEN_GRPC`, `RILL_LISTEN_HTTP`: socket addresses;
    /// * `RILL_DEV_MODE`: `true`/`false`;
    /// * `RILL_STORAGE`: `memory`, `local:<dir>` or `s3:<url>`;
    /// * `RILL_S3_PREFIX`, `RILL_S3_META_DIR`: details for `s3:` storage;
    /// * `RILL_STATIC_TOKENS`: `token=name[:kind[:team,team]];...` entries
    ///   that replace the configured static token list;
    /// * `RILL_ROOT_PUBLIC_KEY`, `RILL_ROOT_PRIVATE_KEY`: hex keys that
    ///   switch authentication to Biscuit;
    /// * `RILL_VERIFY_ENABLED`, `RILL_POSTLAND_ENABLED`: `true`/`false`;
    /// * `RILL_POSTLAND_INTERVAL_SECS`.
    pub fn apply_env(&mut self, lookup: impl Fn(&str) -> Option<String>) -> Result<()> {
        if let Some(v) = lookup("RILL_LISTEN_GRPC") {
            self.listen_grpc = parse_env("RILL_LISTEN_GRPC", &v)?;
        }
        if let Some(v) = lookup("RILL_LISTEN_HTTP") {
            self.listen_http = parse_env("RILL_LISTEN_HTTP", &v)?;
        }
        if let Some(v) = lookup("RILL_DEV_MODE") {
            self.dev_mode = parse_bool("RILL_DEV_MODE", &v)?;
        }
        if let Some(v) = lookup("RILL_STORAGE") {
            self.storage =
                match v.split_once(':') {
                    None if v == "memory" => StorageConfig::Memory,
                    Some(("local", dir)) => StorageConfig::Local {
                        dir: PathBuf::from(dir),
                    },
                    Some(("s3", url)) => StorageConfig::S3 {
                        url: url.to_string(),
                        prefix: lookup("RILL_S3_PREFIX").unwrap_or_default(),
                        meta_dir: lookup("RILL_S3_META_DIR").map(PathBuf::from).ok_or_else(
                            || Error::Config("RILL_S3_META_DIR is required with s3 storage".into()),
                        )?,
                    },
                    _ => {
                        return Err(Error::Config(format!(
                            "RILL_STORAGE: expected memory, local:<dir> or s3:<url>, got {v:?}"
                        )))
                    }
                };
        }
        if let Some(v) = lookup("RILL_STATIC_TOKENS") {
            self.auth = AuthConfig::Static {
                tokens: parse_static_tokens(&v)?,
            };
        }
        if let Some(public) = lookup("RILL_ROOT_PUBLIC_KEY") {
            self.auth = AuthConfig::Biscuit {
                root_public_key_hex: public,
                root_private_key_hex: lookup("RILL_ROOT_PRIVATE_KEY"),
            };
        }
        if let Some(v) = lookup("RILL_VERIFY_ENABLED") {
            self.verify.enabled = parse_bool("RILL_VERIFY_ENABLED", &v)?;
        }
        if let Some(v) = lookup("RILL_POSTLAND_ENABLED") {
            self.postland.enabled = parse_bool("RILL_POSTLAND_ENABLED", &v)?;
        }
        if let Some(v) = lookup("RILL_POSTLAND_INTERVAL_SECS") {
            self.postland.interval_secs = parse_env("RILL_POSTLAND_INTERVAL_SECS", &v)?;
        }
        Ok(())
    }
}

fn parse_env<T: std::str::FromStr>(name: &str, value: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|e| Error::Config(format!("{name}: {e}")))
}

fn parse_bool(name: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(Error::Config(format!(
            "{name}: expected a boolean, got {other:?}"
        ))),
    }
}

/// `token=name[:kind[:team,team]];...`
fn parse_static_tokens(spec: &str) -> Result<Vec<StaticToken>> {
    let mut out = Vec::new();
    for entry in spec.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let (token, rest) = entry.split_once('=').ok_or_else(|| {
            Error::Config(format!("RILL_STATIC_TOKENS: entry {entry:?} lacks '='"))
        })?;
        let mut parts = rest.split(':');
        let name = parts.next().unwrap_or_default();
        let kind = match parts.next() {
            None | Some("human") => PrincipalKind::Human,
            Some("agent") => PrincipalKind::Agent,
            Some("service") => PrincipalKind::Service,
            Some(other) => {
                return Err(Error::Config(format!(
                    "RILL_STATIC_TOKENS: unknown principal kind {other:?}"
                )))
            }
        };
        let teams = parts
            .next()
            .map(|t| t.split(',').map(String::from).collect())
            .unwrap_or_default();
        out.push(StaticToken {
            token: token.to_string(),
            principal_name: name.to_string(),
            kind,
            teams,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn env_overrides() {
        let mut config = ServerConfig::default();
        let env: BTreeMap<&str, &str> = [
            ("RILL_DEV_MODE", "true"),
            ("RILL_STORAGE", "local:/tmp/x"),
            ("RILL_STATIC_TOKENS", "abc=bot:agent:infra,core; def=alice"),
            ("RILL_LISTEN_HTTP", "127.0.0.1:1"),
        ]
        .into_iter()
        .collect();
        config
            .apply_env(|k| env.get(k).map(|v| v.to_string()))
            .unwrap();
        assert!(config.dev_mode);
        assert_eq!(config.listen_http.port(), 1);
        assert_eq!(
            config.storage,
            StorageConfig::Local {
                dir: PathBuf::from("/tmp/x")
            }
        );
        let AuthConfig::Static { tokens } = config.auth else {
            panic!()
        };
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, PrincipalKind::Agent);
        assert_eq!(tokens[0].teams, vec!["infra", "core"]);
        assert_eq!(tokens[1].principal_name, "alice");
        assert!(ServerConfig::default()
            .apply_env(|k| (k == "RILL_DEV_MODE").then(|| "maybe".to_string()))
            .is_err());
    }
}
