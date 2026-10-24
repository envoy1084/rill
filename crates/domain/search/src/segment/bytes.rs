//! A segment's bytes: owned (fetched into memory) or a memory-mapped file
//! from the local segment cache.

use std::ops::Deref;
use std::sync::Arc;

/// The bytes of one segment.
#[derive(Clone)]
pub enum SegmentBytes {
    Owned(bytes::Bytes),
    Mapped(Arc<memmap2::Mmap>),
}

impl Deref for SegmentBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            SegmentBytes::Owned(b) => b,
            SegmentBytes::Mapped(m) => m,
        }
    }
}

impl std::fmt::Debug for SegmentBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SegmentBytes({} bytes)", self.len())
    }
}
