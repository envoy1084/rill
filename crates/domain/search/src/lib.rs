//! Code search for Rill.
//!
//! The index is built from immutable segments ([`segment`]), each covering
//! a batch of blobs by content hash, spread over shards and merged into
//! larger segments over time ([`index`]). Which files a root has is kept as
//! layered path maps ([`paths`]): a base map plus a small delta per
//! landing. A query ([`query`]) turns its pattern into the trigrams every
//! match requires ([`trigram`]), finds candidate blobs in every shard at
//! once, keeps those the root holds at paths the caller may read, verifies
//! them against the stored content and ranks the files. [`symbols`]
//! extracts definitions, stored with each blob.

pub mod error;
pub mod index;
pub mod paths;
pub mod query;
pub mod segment;
pub mod symbols;
pub mod trigram;

pub use error::{Error, Result};
pub use index::{CompactStats, IndexConfig, IndexStats, SearchIndex, Snapshot, Through};
pub use paths::PathView;
pub use query::{
    language_of, search, Facets, LiveFile, PathFilter, Scope, SearchHit, SearchQuery, SearchResult,
};
pub use symbols::{extract_symbols, Symbol, SymbolKind};
pub use trigram::{required_trigrams_for_regex, trigrams, TrigramQuery};
