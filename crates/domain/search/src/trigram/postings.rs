//! Document ids and the posting lists that hold them.

/// Dense identifier of an indexed blob, assigned by the document table in
/// insertion order.
pub type DocId = u32;

/// The documents containing one trigram: a roaring bitmap, compressed and
/// fast to intersect, stored in its portable serialization.
pub type PostingList = roaring::RoaringBitmap;
