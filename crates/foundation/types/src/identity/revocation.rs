//! Revoked tokens.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::ids::TokenId;
use crate::principal::Principal;
use crate::time::Timestamp;

/// A revoked token id. Kept until the token would have expired anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revocation {
    pub token_id: TokenId,
    pub revoked_by: Principal,
    pub revoked_at: Timestamp,
    pub reason: String,
    /// When the revoked token expires; the record can be dropped after.
    pub expires_at: Timestamp,
}

impl Record for Revocation {
    const TAG: &'static str = "revocation";
}
