//! A read grant and its wire form.

use base64::Engine;
use rill_types::{ObjectHash, ObjectKind, PrincipalId, RepoId, RepoPathBuf, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The header (and query parameter) object fetches carry their grant in.
pub const GRANT_HEADER: &str = "x-rill-grant";

/// The longest a grant may live.
pub const MAX_GRANT_TTL_MS: i64 = 10 * 60 * 1000;

/// One object the holder may fetch, and the path it was reached through:
/// a tree's path is where a listing of it continues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantEntry {
    pub kind: ObjectKind,
    pub hash: ObjectHash,
    pub path: RepoPathBuf,
}

/// What a grant says. Signed as a whole by a grant key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadGrant {
    /// The grant key that signed it.
    pub key_id: u32,
    pub principal: PrincipalId,
    pub repo: RepoId,
    pub expires_at: Timestamp,
    pub entries: Vec<GrantEntry>,
}

#[derive(Serialize, Deserialize)]
struct Signed {
    body: Vec<u8>,
    signature: Vec<u8>,
}

const DOMAIN: &[u8] = b"rill:read-grant:";

pub(super) fn message(body: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN.len() + body.len());
    m.extend_from_slice(DOMAIN);
    m.extend_from_slice(body);
    m
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

impl ReadGrant {
    /// The entry naming `(kind, hash)`, if any.
    pub fn entry(&self, kind: ObjectKind, hash: &ObjectHash) -> Option<&GrantEntry> {
        self.entries
            .iter()
            .find(|e| e.kind == kind && e.hash == *hash)
    }

    pub(super) fn body(&self) -> Result<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| Error::Token(format!("grant encoding: {e}")))
    }

    pub(super) fn encode(body: Vec<u8>, signature: Vec<u8>) -> Result<String> {
        let signed = postcard::to_allocvec(&Signed { body, signature })
            .map_err(|e| Error::Token(format!("grant encoding: {e}")))?;
        Ok(b64().encode(signed))
    }

    /// Split a grant into its body and signature, and read the body,
    /// without checking the signature.
    pub(super) fn decode(text: &str) -> Result<(ReadGrant, Vec<u8>, Vec<u8>)> {
        let bad = |why: String| Error::TokenRejected(format!("read grant: {why}"));
        let raw = b64().decode(text.trim()).map_err(|e| bad(e.to_string()))?;
        let signed: Signed = postcard::from_bytes(&raw).map_err(|e| bad(e.to_string()))?;
        let grant: ReadGrant =
            postcard::from_bytes(&signed.body).map_err(|e| bad(e.to_string()))?;
        Ok((grant, signed.body, signed.signature))
    }
}
