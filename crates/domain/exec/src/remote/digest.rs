//! REAPI digests: a BLAKE3 hash and a size, the same id Rill gives raw
//! content (ADR 016).

use bytes::Bytes;
use rill_reapi_proto::reapi::Digest;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::{Error, Result};

/// A validated digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlobDigest {
    pub hash: ObjectHash,
    pub size: u64,
}

impl BlobDigest {
    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        BlobDigest {
            hash: ObjectHash::digest(ObjectKind::Blob, bytes),
            size: bytes.len() as u64,
        }
    }

    /// Parse a wire digest; the hash must be 64 lowercase hex characters.
    pub fn parse(digest: &Digest) -> Result<Self> {
        Self::from_parts(&digest.hash, digest.size_bytes)
    }

    /// Parse a hash and size as they appear in resource names.
    pub fn from_parts(hash: &str, size: i64) -> Result<Self> {
        let valid =
            hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !valid || size < 0 {
            return Err(Error::InvalidDigest(format!("{hash}/{size}")));
        }
        Ok(BlobDigest {
            hash: ObjectHash::from_hex(hash).map_err(|e| Error::InvalidDigest(e.to_string()))?,
            size: size as u64,
        })
    }

    /// Check that `bytes` are the content this digest names.
    pub fn verify(&self, bytes: &Bytes) -> Result<()> {
        if bytes.len() as u64 != self.size
            || ObjectHash::digest(ObjectKind::Blob, bytes) != self.hash
        {
            return Err(Error::DigestMismatch(self.to_string()));
        }
        Ok(())
    }

    pub fn to_proto(self) -> Digest {
        Digest {
            hash: self.hash.to_hex(),
            size_bytes: self.size as i64,
        }
    }

    /// Whether this is the digest of empty content, which every store has.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

impl std::fmt::Display for BlobDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.hash, self.size)
    }
}
