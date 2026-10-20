//! The annotated configuration file `rill-server init-config` writes.
//!
//! The body is the serialized default configuration, so every key and
//! default is exactly what the server would use; the comments explain the
//! sections and the alternatives that the defaults do not show.

use rill_server::ServerConfig;

const HEADER: &str = r#"# rill-server configuration.
#
# Every key is optional; a missing key takes the default shown here. The
# RILL_* environment variables override individual keys:
#   RILL_LISTEN_GRPC, RILL_LISTEN_HTTP   socket addresses
#   RILL_DEV_MODE                        true/false
#   RILL_STORAGE                         memory | local:<dir> | s3:<url>
#   RILL_S3_PREFIX, RILL_S3_META_DIR     details for s3 storage
#   RILL_STATIC_TOKENS                   token=name[:kind[:team,team]];...
#   RILL_ROOT_PUBLIC_KEY, RILL_ROOT_PRIVATE_KEY   switch auth to biscuit
#   RILL_VERIFY_ENABLED, RILL_POSTLAND_ENABLED, RILL_POSTLAND_INTERVAL_SECS
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
#   meta_dir = "/var/lib/rill/meta"
#
# Authentication alternatives:
#   [auth]
#   kind = "static"
#   [[auth.tokens]]
#   token = "change-me"
#   principal_name = "alice"
#   kind = "human"            # human | agent | service
#   teams = ["core"]
#
#   [auth]
#   kind = "biscuit"
#   root_public_key_hex = "..."
#   root_private_key_hex = "..."   # optional; lets this server issue tokens
#
# dev_mode = true disables authentication and authorization entirely.
# Only ever use it on a loopback address.

"#;

/// The annotated default configuration as TOML text.
pub fn annotated_config() -> anyhow::Result<String> {
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
