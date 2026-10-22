//! Credentials provisioning systems (Okta, Entra ID) call SCIM with.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::principal::Principal;
use crate::time::Timestamp;

/// A long-lived SCIM bearer credential of one organization. Provisioning
/// systems cannot refresh tokens, so these do not expire; they are revoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScimCredential {
    pub org: String,
    pub id: String,
    /// BLAKE3 of the secret.
    pub secret_hash: [u8; 32],
    pub created_by: Principal,
    pub created_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

impl Record for ScimCredential {
    const TAG: &'static str = "scim-credential";
}
