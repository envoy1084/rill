//! Trigrams: extraction from bytes, extraction of the trigrams a regex
//! requires, and the posting-list encoding.

mod extract;
mod literals;
mod postings;
mod query;

pub use extract::{fold_byte, pack, trigrams, unpack};
pub use literals::required_trigrams_for_regex;
pub use postings::{DocId, PostingList};
pub use query::{Candidates, TrigramQuery};
