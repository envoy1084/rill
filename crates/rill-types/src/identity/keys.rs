//! Token signing keys as stored: the public half in clear, the secret
//! wrapped by a key service.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::time::Timestamp;

/// A secret encrypted by a key service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedSecret {
    /// The key service that wrapped it (`local`, `vault`, `aws-kms`,
    /// `gcp-kms`).
    pub backend: String,
    /// The wrapping key within that service.
    pub key_ref: String,
    pub ciphertext: Vec<u8>,
}

/// One P-256 token signing key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenKeyRecord {
    pub id: u32,
    /// Compressed SEC1 public key.
    pub public_key: Vec<u8>,
    /// The public half of the read-grant key derived from this key's
    /// secret; grants carry this record's id.
    pub grant_public_key: Vec<u8>,
    pub secret: WrappedSecret,
    pub created_at: Timestamp,
    /// Servers sign with the newest active key; verifiers learn a key
    /// before it signs anything.
    pub active_from: Timestamp,
    /// Set when a newer key takes over: tokens signed by this key are
    /// refused from then on.
    pub not_after: Option<Timestamp>,
}

impl Record for TokenKeyRecord {
    const TAG: &'static str = "tokenkey";
}
