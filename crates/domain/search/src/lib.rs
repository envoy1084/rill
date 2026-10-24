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

mod cpu;
pub mod error;
pub mod index;
pub mod paths;
pub mod query;
pub mod scip;
pub mod segment;
pub mod structural;
pub mod symbols;
pub mod trigram;

pub use error::{Error, Result};
pub use index::{CompactStats, IndexConfig, IndexStats, SearchIndex, Snapshot, Through};
pub use paths::PathView;
pub use query::{
    assemble, language_of, search, search_shards, Facets, LiveFile, PathFilter, RankedFile, Scope,
    SearchHit, SearchQuery, SearchResult, ShardResults, ShardSelect,
};
pub use scip::{scip_for, store_scip, ScipIndex, ScipLocation, ScipView};
pub use structural::{structural_languages, StructuralPattern};
pub use symbols::{
    extract_symbols, file_symbols, find_symbols, Symbol, SymbolHit, SymbolKind, SymbolName,
    SymbolQuery,
};
pub use trigram::{required_trigrams_for_regex, trigrams, TrigramQuery};
