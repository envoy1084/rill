//! Trigrams: extraction from bytes, extraction of the trigrams a regex
//! requires, and the queries over their posting lists.

mod extract;
mod literals;
mod query;

pub use extract::{fold_byte, pack, trigrams, unpack};
pub use literals::required_trigrams_for_regex;
/// Dense identifier of an indexed blob, assigned by the document table in
/// insertion order.
pub type DocId = u32;

/// The documents containing one trigram: a roaring bitmap, compressed and
/// fast to intersect, stored in its portable serialization.
pub type PostingList = roaring::RoaringBitmap;
pub use query::{Candidates, TrigramQuery};
