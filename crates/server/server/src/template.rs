//! The annotated configuration file `rill-server init-config` writes.
//!
//! The body is the serialized default configuration, so every key and
//! default is exactly what the server would use; the comments explain the
//! sections and the alternatives that the defaults do not show.

use rill_api::ServerConfig;

const HEADER: &str = r#"# rill-server configuration.
#
# Every key is optional; a missing key takes the default shown here. The
# RILL_* environment variables override individual keys:
#   RILL_LISTEN_GRPC, RILL_LISTEN_HTTP   socket addresses
#   RILL_DEV_MODE                        true/false
#   RILL_ROLES                           api | workers | api,workers
#   RILL_STORAGE                         memory | local:<dir> | s3:<url>
#   RILL_S3_PREFIX, RILL_S3_META_DIR     details for s3 storage
#   RILL_STATIC_TOKENS                   token=name[:kind[:team,team]];... (dev mode)
#   RILL_MASTER_KEY, RILL_MASTER_KEY_FILE         local key service
#   RILL_VAULT_ADDR + RILL_VAULT_KEY (VAULT_TOKEN) Vault Transit
#   RILL_AWS_KMS_KEY, RILL_GCP_KMS_KEY           cloud KMS (aws / gcp builds)
#   RILL_ADMINS                          instance administrators, comma-separated
#   RILL_VERIFY_ENABLED, RILL_POSTLAND_ENABLED, RILL_POSTLAND_INTERVAL_SECS,
#   RILL_EXEC_LOCAL_WORKERS
#
# Storage alternatives:
#   [storage]
#   kind = "local"            # objects as files, metadata in redb
#   dir = "/var/lib/rill"
#
#   [storage]
#   kind = "s3"
#   url = "s3://bucket"
#   prefix = "rill/"
#   meta_dir = "/var/lib/rill/meta"   # not needed with FoundationDB metadata
#
# Metadata in FoundationDB (server built with --features fdb):
#   [metadata]
#   kind = "fdb"
#   cluster_file = "/etc/foundationdb/fdb.cluster"   # optional
#   root = "rill"                                     # subspace name
#
# Every token is signed by the instance's P-256 keys, which the key
# service wraps and the metadata store keeps. Key service alternatives:
#   [identity.keys]
#   backend = "local"
#   master_key_file = "/etc/rill/master.key"   # 64 hex characters
#
#   [identity.keys]
#   backend = "vault"
#   address = "https://vault:8200"
#   key = "rill"                 # a Transit key; token from VAULT_TOKEN
#
#   [identity.keys]
#   backend = "aws-kms"          # or gcp-kms with key_name = "projects/..."
#   key_id = "arn:aws:kms:..."
#
# The first administrator signs in with a token from
#   rill-server mint-token human:<name> --config rill.toml
# and is listed in identity.admins.
#
# Static tokens are accepted in dev mode only:
#   [[auth.tokens]]
#   token = "change-me"
#   principal_name = "alice"
#   kind = "Human"            # Human | Agent | Service
#   teams = ["core"]
#
# dev_mode = true disables authentication and authorization entirely.
# Only ever use it on a loopback address.

"#;

/// The annotated default configuration as TOML text.
pub fn annotated_config() -> crate::Result<String> {
    let body = ServerConfig::default().to_toml()?;
    Ok(format!("{HEADER}{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips_to_the_default() {
        let text = annotated_config().unwrap();
        let parsed = ServerConfig::from_toml(&text).unwrap();
        assert_eq!(parsed, ServerConfig::default());
    }
}
