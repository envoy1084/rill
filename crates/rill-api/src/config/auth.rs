//! How bearer tokens are authenticated.

use rill_types::PrincipalKind;
use serde::{Deserialize, Serialize};

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
