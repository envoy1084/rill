//! What the overlay records per path.

use rill_types::{FileKind, ObjectHash};
use serde::{Deserialize, Serialize};

/// The local state of one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayOp {
    /// The path holds this content (keyed by its local digest).
    Set {
        /// Local digest of the bytes, the object cache key.
        digest: ObjectHash,
        /// File kind.
        kind: FileKind,
        /// Byte length.
        size: u64,
    },
    /// The path is absent.
    Delete,
}

/// One overlay entry: the latest operation on a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayEntry {
    /// The path's local state.
    pub op: OverlayOp,
    /// Order of recording; entries above the mount's synced sequence are
    /// still to be pushed.
    pub seq: u64,
    /// The content holds conflict markers from an update.
    pub conflict: bool,
}

impl OverlayEntry {
    /// The local digest, for a set.
    pub fn digest(&self) -> Option<ObjectHash> {
        match self.op {
            OverlayOp::Set { digest, .. } => Some(digest),
            OverlayOp::Delete => None,
        }
    }
}
