use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The kind of object a hash refers to.
///
/// Kinds are mixed into the hash as a domain separator, so a blob whose bytes
/// happen to equal a tree's encoding can never collide with that tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ObjectKind {
    Blob = 1,
    Chunk = 2,
    BlobManifest = 3,
    Tree = 4,
    ChangeVersion = 5,
    Derived = 6,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Blob => "blob",
            ObjectKind::Chunk => "chunk",
            ObjectKind::BlobManifest => "manifest",
            ObjectKind::Tree => "tree",
            ObjectKind::ChangeVersion => "cv",
            ObjectKind::Derived => "derived",
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(ObjectKind::Blob),
            2 => Some(ObjectKind::Chunk),
            3 => Some(ObjectKind::BlobManifest),
            4 => Some(ObjectKind::Tree),
            5 => Some(ObjectKind::ChangeVersion),
            6 => Some(ObjectKind::Derived),
            _ => None,
        }
    }
}

impl FromStr for ObjectKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "blob" => Ok(ObjectKind::Blob),
            "chunk" => Ok(ObjectKind::Chunk),
            "manifest" => Ok(ObjectKind::BlobManifest),
            "tree" => Ok(ObjectKind::Tree),
            "cv" => Ok(ObjectKind::ChangeVersion),
            "derived" => Ok(ObjectKind::Derived),
            other => Err(Error::KindMismatch {
                expected: "known object kind".into(),
                found: other.into(),
            }),
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A 256-bit BLAKE3 content hash identifying an immutable object.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObjectHash([u8; 32]);

impl ObjectHash {
    pub const LEN: usize = 32;

    /// A hash of all zero bytes. Never produced by hashing; used as a sentinel
    /// for "empty" where a real object is not required.
    pub const ZERO: ObjectHash = ObjectHash([0u8; 32]);

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ObjectHash(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Error::InvalidHash(format!("expected 32 bytes, got {}", bytes.len())))?;
        Ok(ObjectHash(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|e| Error::InvalidHash(e.to_string()))?;
        Self::from_slice(&bytes)
    }

    /// Short prefix suitable for logs and UI (12 hex chars).
    pub fn short(&self) -> String {
        self.to_hex()[..12].to_string()
    }

    /// Hash raw bytes under the domain separator for `kind`.
    pub fn digest(kind: ObjectKind, bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rill:");
        hasher.update(kind.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(bytes);
        ObjectHash(*hasher.finalize().as_bytes())
    }

    /// Hash a large payload using BLAKE3's multithreaded tree hashing.
    pub fn digest_parallel(kind: ObjectKind, bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rill:");
        hasher.update(kind.as_str().as_bytes());
        hasher.update(b":");
        hasher.update_rayon(bytes);
        ObjectHash(*hasher.finalize().as_bytes())
    }

    /// Streaming hasher for content that does not fit in memory.
    pub fn hasher(kind: ObjectKind) -> Hasher {
        let mut inner = blake3::Hasher::new();
        inner.update(b"rill:");
        inner.update(kind.as_str().as_bytes());
        inner.update(b":");
        Hasher { inner }
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

/// Incremental hasher returned by [`ObjectHash::hasher`].
pub struct Hasher {
    inner: blake3::Hasher,
}

impl Hasher {
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.inner.update(bytes);
        self
    }

    pub fn finalize(self) -> ObjectHash {
        ObjectHash(*self.inner.finalize().as_bytes())
    }
}

impl fmt::Debug for ObjectHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectHash({})", self.short())
    }
}

impl fmt::Display for ObjectHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for ObjectHash {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_hex(s)
    }
}

impl AsRef<[u8]> for ObjectHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_separate_domains() {
        let a = ObjectHash::digest(ObjectKind::Blob, b"x");
        let b = ObjectHash::digest(ObjectKind::Tree, b"x");
        assert_ne!(a, b);
    }

    #[test]
    fn hex_roundtrip() {
        let h = ObjectHash::digest(ObjectKind::Blob, b"hello");
        assert_eq!(ObjectHash::from_hex(&h.to_hex()).unwrap(), h);
        assert_eq!(h.short().len(), 12);
    }

    #[test]
    fn streaming_matches_oneshot() {
        let mut h = ObjectHash::hasher(ObjectKind::Blob);
        h.update(b"hel").update(b"lo");
        assert_eq!(h.finalize(), ObjectHash::digest(ObjectKind::Blob, b"hello"));
    }
}
