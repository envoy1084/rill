//! What a query matches with: a regex over bytes, or a structural pattern
//! over syntax trees.

use std::ops::Range;

use regex::bytes::Regex;

use crate::error::Result;
use crate::structural::StructuralPattern;
use crate::trigram::{required_trigrams_for_regex, TrigramQuery};

/// A compiled query pattern.
#[derive(Debug, Clone)]
pub enum Matcher {
    Regex { regex: Regex, source: String },
    Structural(StructuralPattern),
}

impl Matcher {
    /// The trigram query a matching file satisfies. The index folds case
    /// regardless of the query, so literals are always folded here.
    pub fn trigram_query(&self) -> Result<TrigramQuery> {
        match self {
            Matcher::Regex { source, .. } => required_trigrams_for_regex(source, true),
            Matcher::Structural(p) => Ok(p.trigram_query()),
        }
    }

    /// Byte ranges of the matches in the file at `path`, in order.
    pub fn find<'a>(
        &'a self,
        path: &str,
        bytes: &'a [u8],
    ) -> Box<dyn Iterator<Item = Range<usize>> + 'a> {
        match self {
            Matcher::Regex { regex, .. } => Box::new(regex.find_iter(bytes).map(|m| m.range())),
            Matcher::Structural(p) => Box::new(p.find(path, bytes).into_iter()),
        }
    }

    /// Whether a file name matches (a ranking signal; regex only).
    pub fn matches_name(&self, name: &str) -> bool {
        match self {
            Matcher::Regex { regex, .. } => regex.is_match(name.as_bytes()),
            Matcher::Structural(_) => false,
        }
    }
}
