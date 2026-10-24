//! Segments: immutable index files, each covering a batch of blobs.
//!
//! A segment holds a trigram dictionary (an `fst` map from trigram to the
//! place of its posting list), the posting lists (roaring bitmaps of the
//! segment's own document numbers), each blob's content compressed with
//! lz4 so a candidate is verified without fetching the blob, and each
//! blob's symbols. Segments never change once written; the index grows by
//! adding small segments and merging them into larger ones.

mod bytes;
mod codec;
mod format;
mod merge;
mod reader;
mod writer;

pub use self::bytes::SegmentBytes;
pub use format::SegmentDoc;
pub use merge::merge_segments;
pub use reader::Segment;
pub use writer::{PreparedDoc, SegmentBuilder};
