//! Code search for Rill.
//!
//! The index is keyed by blob hash, so a blob that is unchanged between
//! versions is never tokenized twice (the idea behind GitHub's Blackbird).
//! Two layers make that work:
//!
//! 1. A trigram index over blob content ([`trigram`], [`index`]): every
//!    blob gets a dense document id and its trigrams point at it.
//! 2. A manifest per root tree ([`index::RootManifest`]): the paths of a
//!    root and the content behind each.
//!
//! A query ([`query`]) turns the regex into the trigrams it requires,
//! intersects their posting lists into candidate documents, runs the regex
//! over the candidate blobs of the requested root, and maps hits back to
//! paths. [`symbols`] adds definition lookup on top of the same trees.

pub mod error;
pub mod index;
pub mod query;
pub mod symbols;
pub mod trigram;

pub use error::{Error, Result};
pub use index::{FlushStats, IndexConfig, IndexStats, RootManifest, SearchIndex};
pub use query::{search, PathFilter, SearchHit, SearchQuery};
pub use symbols::{extract_symbols, RootSymbols, Symbol, SymbolIndex, SymbolKind, SymbolLocation};
pub use trigram::{required_trigrams_for_regex, trigrams, TrigramQuery};
