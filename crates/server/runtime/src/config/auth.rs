//! Static bearer tokens, for development servers only.

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

/// Static tokens. They let tools act as named principals on a dev-mode
/// server; outside dev mode the server refuses to start with any, and
/// every caller presents a token the instance signed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub tokens: Vec<StaticToken>,
}
