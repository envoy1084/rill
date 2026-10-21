//! OpenID Connect identity providers an organization signs in through.

use serde::{Deserialize, Serialize};

use super::keys::WrappedSecret;
use crate::encoding::Record;
use crate::time::Timestamp;

/// One identity provider of an organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityProvider {
    pub org: String,
    pub name: String,
    /// The OIDC issuer; discovery is at `<issuer>/.well-known/openid-configuration`.
    pub issuer: String,
    /// The confidential client browser sign-in uses.
    pub client_id: String,
    /// Its secret, wrapped by the instance's key service.
    pub client_secret: Option<WrappedSecret>,
    /// A public client the CLI uses for the device flow; ID tokens issued
    /// to it are accepted too.
    pub cli_client_id: Option<String>,
    pub scopes: Vec<String>,
    /// The claim that names the user (`preferred_username`, `email`).
    pub username_claim: String,
    /// The claim listing the user's groups, which become teams.
    pub groups_claim: Option<String>,
    /// Whether a first sign-in creates the user; otherwise provisioning
    /// must have created it.
    pub auto_provision: bool,
    pub created_at: Timestamp,
}

impl Record for IdentityProvider {
    const TAG: &'static str = "identity-provider";
}

impl IdentityProvider {
    /// Client ids whose ID tokens this provider accepts.
    pub fn audiences(&self) -> Vec<String> {
        let mut out = vec![self.client_id.clone()];
        out.extend(self.cli_client_id.clone());
        out
    }
}
