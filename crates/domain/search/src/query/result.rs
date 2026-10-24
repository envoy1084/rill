//! What a search returns.

use std::collections::BTreeMap;

use super::hit::SearchHit;

/// Matched files counted by top-level directory and by language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    pub directories: BTreeMap<String, u32>,
    pub languages: BTreeMap<String, u32>,
}

/// The hits of one search, best files first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// Files with at least one match (among those looked at).
    pub files: u32,
    /// More hits exist than were returned.
    pub truncated: bool,
    /// The time budget ran out or the query could not be narrowed, so not
    /// every candidate was looked at.
    pub partial: bool,
    pub facets: Facets,
}
