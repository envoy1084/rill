//! `RILL_*` environment overrides.

use std::path::PathBuf;

use rill_types::PrincipalKind;

use super::{AuthConfig, MetadataConfig, ServerConfig, StaticToken, StorageConfig};
use crate::error::{Error, Result};

impl ServerConfig {
    /// Apply overrides from `lookup`, which answers environment variable
    /// names. Recognised variables:
    ///
    /// * `RILL_LISTEN_GRPC`, `RILL_LISTEN_HTTP`: socket addresses;
    /// * `RILL_DEV_MODE`: `true`/`false`;
    /// * `RILL_ENCRYPTION`: `true`/`false`, sealing objects at rest;
    /// * `RILL_STRUCTURAL_MERGE`: `true`/`false`, syntax-aware merge of text
    ///   conflicts;
    /// * `RILL_ROLES`: `api`, `workers` or `api,workers`;
    /// * `RILL_LOG_FORMAT`: `text` or `json`;
    /// * `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`: trace export;
    /// * `RILL_TLS_CERT`, `RILL_TLS_KEY`, `RILL_TLS_CLIENT_CA`: PEM files
    ///   for TLS on both listeners (the CA makes it mutual);
    /// * `RILL_STORAGE`: `memory`, `local:<dir>` or `s3:<url>`;
    /// * `RILL_S3_PREFIX`, `RILL_S3_META_DIR`: details for `s3:` storage;
    /// * `RILL_FDB_CLUSTER_FILE` (empty for the default), `RILL_FDB_ROOT`:
    ///   keep metadata in FoundationDB;
    /// * `RILL_STATIC_TOKENS`: `token=name[:kind[:team,team]];...` entries
    ///   that replace the configured static token list (dev mode only);
    /// * `RILL_MASTER_KEY`, `RILL_MASTER_KEY_FILE`, `RILL_VAULT_ADDR` with
    ///   `RILL_VAULT_KEY`, `RILL_AWS_KMS_KEY`, `RILL_GCP_KMS_KEY`: the key
    ///   service that wraps token keys;
    /// * `RILL_ADMINS`: comma-separated instance administrator names;
    /// * `RILL_VERIFY_ENABLED`, `RILL_POSTLAND_ENABLED`, `RILL_EXEC_ENABLED`: `true`/`false`;
    /// * `RILL_EXEC_WORKER_SECRET_FILE`: the file of worker secrets;
    /// * `RILL_POSTLAND_INTERVAL_SECS`.
    pub fn apply_env(&mut self, lookup: impl Fn(&str) -> Option<String>) -> Result<()> {
        if let Some(v) = lookup("RILL_LISTEN_GRPC") {
            self.listen_grpc = parse_env("RILL_LISTEN_GRPC", &v)?;
        }
        if let Some(v) = lookup("RILL_LISTEN_HTTP") {
            self.listen_http = parse_env("RILL_LISTEN_HTTP", &v)?;
        }
        if let Some(cert) = lookup("RILL_TLS_CERT") {
            let key = lookup("RILL_TLS_KEY")
                .ok_or_else(|| Error::Config("RILL_TLS_CERT needs RILL_TLS_KEY".into()))?;
            self.tls = Some(super::TlsConfig {
                cert: PathBuf::from(cert),
                key: PathBuf::from(key),
                client_ca: lookup("RILL_TLS_CLIENT_CA").map(PathBuf::from),
            });
        }
        if let Some(v) = lookup("RILL_LOG_FORMAT") {
            self.telemetry.log_format = match v.as_str() {
                "text" => super::LogFormat::Text,
                "json" => super::LogFormat::Json,
                other => {
                    return Err(Error::Config(format!(
                        "RILL_LOG_FORMAT: expected text or json, got {other:?}"
                    )))
                }
            };
        }
        if let Some(v) = lookup("OTEL_EXPORTER_OTLP_ENDPOINT").filter(|v| !v.is_empty()) {
            self.telemetry.otlp_endpoint = Some(v);
        }
        if let Some(v) = lookup("OTEL_SERVICE_NAME").filter(|v| !v.is_empty()) {
            self.telemetry.service_name = v;
        }
        if let Some(v) = lookup("RILL_ROLES") {
            self.roles = super::Roles::parse(&v)?;
        }
        if let Some(v) = lookup("RILL_DEV_MODE") {
            self.dev_mode = parse_bool("RILL_DEV_MODE", &v)?;
        }
        if let Some(v) = lookup("RILL_STRUCTURAL_MERGE") {
            self.merge.structural = parse_bool("RILL_STRUCTURAL_MERGE", &v)?;
        }
        if let Some(v) = lookup("RILL_ENCRYPTION") {
            self.encryption.enabled = Some(parse_bool("RILL_ENCRYPTION", &v)?);
        }
        if let Some(v) = lookup("RILL_STORAGE") {
            self.storage = match v.split_once(':') {
                None if v == "memory" => StorageConfig::Memory,
                Some(("local", dir)) => StorageConfig::Local {
                    dir: PathBuf::from(dir),
                },
                Some(("s3", url)) => StorageConfig::S3 {
                    url: url.to_string(),
                    prefix: lookup("RILL_S3_PREFIX").unwrap_or_default(),
                    meta_dir: lookup("RILL_S3_META_DIR").map(PathBuf::from),
                    allow_unconditional_put: lookup("RILL_S3_ALLOW_UNCONDITIONAL_PUT")
                        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
                },
                _ => {
                    return Err(Error::Config(format!(
                        "RILL_STORAGE: expected memory, local:<dir> or s3:<url>, got {v:?}"
                    )))
                }
            };
        }
        if let Some(v) = lookup("RILL_FDB_CLUSTER_FILE") {
            self.metadata = MetadataConfig::Fdb {
                cluster_file: (!v.is_empty()).then(|| PathBuf::from(v)),
                root: lookup("RILL_FDB_ROOT").unwrap_or_else(|| "rill".into()),
            };
        }
        if let Some(v) = lookup("RILL_STATIC_TOKENS") {
            self.auth = AuthConfig {
                tokens: parse_static_tokens(&v)?,
            };
        }
        self.apply_key_service_env(&lookup);
        if let Some(v) = lookup("RILL_ADMINS") {
            self.identity.admins = v
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(String::from)
                .collect();
        }
        if let Some(v) = lookup("RILL_VERIFY_ENABLED") {
            self.verify.enabled = parse_bool("RILL_VERIFY_ENABLED", &v)?;
        }
        if let Some(v) = lookup("RILL_EXEC_ENABLED") {
            self.exec.enabled = parse_bool("RILL_EXEC_ENABLED", &v)?;
        }
        if let Some(v) = lookup("RILL_EXEC_WORKER_SECRET_FILE") {
            self.exec.worker_secret_file = (!v.is_empty()).then(|| PathBuf::from(v));
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

impl ServerConfig {
    /// The key service from `RILL_MASTER_KEY` / `RILL_MASTER_KEY_FILE`
    /// (local), `RILL_VAULT_ADDR` + `RILL_VAULT_KEY`, `RILL_AWS_KMS_KEY`
    /// (+ `RILL_AWS_KMS_ENDPOINT`, `AWS_REGION`) or `RILL_GCP_KMS_KEY`.
    fn apply_key_service_env(&mut self, lookup: &impl Fn(&str) -> Option<String>) {
        use rill_identity::keys::KeyServiceConfig;
        let master_hex = lookup("RILL_MASTER_KEY");
        let master_file = lookup("RILL_MASTER_KEY_FILE").map(PathBuf::from);
        if master_hex.is_some() || master_file.is_some() {
            self.identity.keys = KeyServiceConfig::Local {
                master_key_hex: master_hex,
                master_key_file: master_file,
            };
        }
        if let (Some(address), Some(key)) = (lookup("RILL_VAULT_ADDR"), lookup("RILL_VAULT_KEY")) {
            self.identity.keys = KeyServiceConfig::Vault {
                address,
                key,
                mount: lookup("RILL_VAULT_MOUNT").unwrap_or_else(|| "transit".into()),
                token: None,
                token_file: lookup("RILL_VAULT_TOKEN_FILE").map(PathBuf::from),
            };
        }
        if let Some(key_id) = lookup("RILL_AWS_KMS_KEY") {
            self.identity.keys = KeyServiceConfig::AwsKms {
                key_id,
                region: lookup("AWS_REGION"),
                endpoint: lookup("RILL_AWS_KMS_ENDPOINT"),
            };
        }
        if let Some(key_name) = lookup("RILL_GCP_KMS_KEY") {
            self.identity.keys = KeyServiceConfig::GcpKms {
                key_name,
                endpoint: None,
            };
        }
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
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn env_overrides() {
        let mut config = ServerConfig::default();
        let env: BTreeMap<&str, &str> = [
            ("RILL_DEV_MODE", "true"),
            ("RILL_STORAGE", "local:/tmp/x"),
            ("RILL_STATIC_TOKENS", "abc=bot:agent:infra,core; def=alice"),
            ("RILL_LISTEN_HTTP", "127.0.0.1:1"),
            ("RILL_FDB_CLUSTER_FILE", ""),
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
        let tokens = config.auth.tokens;
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, PrincipalKind::Agent);
        assert_eq!(tokens[0].teams, vec!["infra", "core"]);
        assert_eq!(tokens[1].principal_name, "alice");
        assert_eq!(
            config.metadata,
            MetadataConfig::Fdb {
                cluster_file: None,
                root: "rill".into()
            }
        );
        assert!(ServerConfig::default()
            .apply_env(|k| (k == "RILL_DEV_MODE").then(|| "maybe".to_string()))
            .is_err());
    }
}
