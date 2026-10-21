//! Choosing a key service from configuration.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::local::LocalKeyService;
use super::vault::VaultTransit;
use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

/// The key service that wraps token signing keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "kebab-case")]
pub enum KeyServiceConfig {
    /// A 32-byte AES-256-GCM master key, as hex or in a file holding hex.
    /// With neither, a random master key lives only in this process.
    Local {
        #[serde(default)]
        master_key_hex: Option<String>,
        #[serde(default)]
        master_key_file: Option<PathBuf>,
    },
    /// A Vault Transit key. The token comes from `token`, `token_file` or
    /// `VAULT_TOKEN`.
    Vault {
        address: String,
        key: String,
        #[serde(default = "default_mount")]
        mount: String,
        #[serde(default)]
        token: Option<String>,
        #[serde(default)]
        token_file: Option<PathBuf>,
    },
    /// An AWS KMS symmetric key (needs the `aws` feature).
    AwsKms {
        key_id: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// A Google Cloud KMS key, `projects/…/cryptoKeys/…` (needs the `gcp`
    /// feature).
    GcpKms {
        key_name: String,
        #[serde(default)]
        endpoint: Option<String>,
    },
}

fn default_mount() -> String {
    "transit".into()
}

impl Default for KeyServiceConfig {
    fn default() -> Self {
        KeyServiceConfig::Local {
            master_key_hex: None,
            master_key_file: None,
        }
    }
}

fn read_secret(path: &PathBuf) -> Result<String> {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))
}

/// Build the key service `config` names.
pub async fn key_service(config: &KeyServiceConfig) -> Result<Arc<dyn KeyWrapper>> {
    match config {
        KeyServiceConfig::Local {
            master_key_hex,
            master_key_file,
        } => {
            let hex_key = match (master_key_hex, master_key_file) {
                (Some(hex_key), _) => Some(hex_key.clone()),
                (None, Some(path)) => Some(read_secret(path)?),
                (None, None) => None,
            };
            match hex_key {
                Some(hex_key) => {
                    let bytes = hex::decode(hex_key.trim())
                        .map_err(|e| Error::Config(format!("local master key: {e}")))?;
                    Ok(Arc::new(LocalKeyService::new(&bytes)?))
                }
                None => {
                    tracing::warn!(
                        "no master key configured; token keys are wrapped with a per-process key"
                    );
                    Ok(Arc::new(LocalKeyService::generate()))
                }
            }
        }
        KeyServiceConfig::Vault {
            address,
            key,
            mount,
            token,
            token_file,
        } => {
            let token = match (token, token_file) {
                (Some(token), _) => token.clone(),
                (None, Some(path)) => read_secret(path)?,
                (None, None) => std::env::var("VAULT_TOKEN").map_err(|_| {
                    Error::Config("vault needs token, token_file or VAULT_TOKEN".into())
                })?,
            };
            Ok(Arc::new(VaultTransit::new(address, &token, mount, key)))
        }
        #[cfg(feature = "aws")]
        KeyServiceConfig::AwsKms {
            key_id,
            region,
            endpoint,
        } => Ok(Arc::new(
            super::aws::AwsKms::connect(key_id, region.as_deref(), endpoint.as_deref()).await,
        )),
        #[cfg(not(feature = "aws"))]
        KeyServiceConfig::AwsKms { .. } => Err(Error::Config(
            "AWS KMS needs a build with the `aws` feature".into(),
        )),
        #[cfg(feature = "gcp")]
        KeyServiceConfig::GcpKms { key_name, endpoint } => Ok(Arc::new(
            super::gcp::GcpKms::connect(key_name, endpoint.as_deref()).await?,
        )),
        #[cfg(not(feature = "gcp"))]
        KeyServiceConfig::GcpKms { .. } => Err(Error::Config(
            "Google Cloud KMS needs a build with the `gcp` feature".into(),
        )),
    }
}
