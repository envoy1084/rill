//! Blob references and chunked blob manifests.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::encoding::{Encodable, FormatVersion};
use crate::hash::{ObjectHash, ObjectKind};

/// Files at or below this size are stored inline in the change that adds
/// them, avoiding a blob round-trip for the long tail of tiny files.
pub const INLINE_THRESHOLD: usize = 256;

/// Files at or above this size are chunked with FastCDC and stored as a
/// manifest plus chunks.
pub const CHUNK_THRESHOLD: u64 = 1024 * 1024;

/// Reference to file content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContentRef {
    /// Content stored inline; `bytes.len() <= INLINE_THRESHOLD`.
    Inline { bytes: Bytes },
    /// A single blob object.
    Blob { hash: ObjectHash, size: u64 },
    /// A chunked blob: `hash` identifies a [`BlobManifest`].
    Chunked { hash: ObjectHash, size: u64 },
}

impl ContentRef {
    pub fn inline(bytes: impl Into<Bytes>) -> Self {
        ContentRef::Inline {
            bytes: bytes.into(),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            ContentRef::Inline { bytes } => bytes.len() as u64,
            ContentRef::Blob { size, .. } | ContentRef::Chunked { size, .. } => *size,
        }
    }

    /// Content identity usable as a dedup key: the blob or manifest hash, or
    /// a hash of the inline bytes.
    pub fn content_hash(&self) -> ObjectHash {
        match self {
            ContentRef::Inline { bytes } => ObjectHash::digest(ObjectKind::Blob, bytes),
            ContentRef::Blob { hash, .. } | ContentRef::Chunked { hash, .. } => *hash,
        }
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, ContentRef::Inline { .. })
    }

    /// Hash to fetch from the object store, if the content is not inline.
    pub fn stored_hash(&self) -> Option<(ObjectKind, ObjectHash)> {
        match self {
            ContentRef::Inline { .. } => None,
            ContentRef::Blob { hash, .. } => Some((ObjectKind::Blob, *hash)),
            ContentRef::Chunked { hash, .. } => Some((ObjectKind::BlobManifest, *hash)),
        }
    }
}

/// One chunk of a chunked blob.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkRef {
    pub hash: ObjectHash,
    pub size: u32,
}

/// Ordered list of chunks that concatenate to a file's content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobManifest {
    pub total_size: u64,
    pub chunks: Vec<ChunkRef>,
}

impl Encodable for BlobManifest {
    const KIND: ObjectKind = ObjectKind::BlobManifest;
    const VERSION: FormatVersion = 1;
    const NAME: &'static str = "BlobManifest";
}
