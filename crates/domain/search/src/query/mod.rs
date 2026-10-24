//! Running searches over an indexed root, or an indexed root plus files
//! read live.

mod hit;
mod lang;
mod lines;
mod matcher;
pub(crate) mod rank;
mod request;
mod result;
mod scope;
mod search;

pub use hit::SearchHit;
pub use lang::language_of;
pub use lines::LineIndex;
pub use matcher::Matcher;
pub use request::{SearchQuery, DEFAULT_MAX_RESULTS, DEFAULT_TIME_BUDGET};
pub use result::{Facets, SearchResult};
pub use scope::{LiveFile, Scope};
pub use search::{
    assemble, search, search_shards, PathFilter, RankedFile, ShardResults, ShardSelect,
};
