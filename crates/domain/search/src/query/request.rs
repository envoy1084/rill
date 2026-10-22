//! Search parameters.

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::bytes::{Regex, RegexBuilder};

use crate::error::Result;
use crate::trigram::{required_trigrams_for_regex, TrigramQuery};

/// Default cap on returned hits.
pub const DEFAULT_MAX_RESULTS: usize = 100;

/// A regex search over one root.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Regex in `regex` crate syntax, matched against raw bytes.
    pub pattern: String,
    /// Ignore ASCII and Unicode case.
    pub case_insensitive: bool,
    /// Only files whose path matches one of these globs are searched.
    pub path_glob: Option<GlobSet>,
    /// Stop after this many hits.
    pub max_results: usize,
    /// Lines of context on each side of a hit.
    pub context_lines: usize,
}

impl SearchQuery {
    /// A case-sensitive search for `pattern` with default limits.
    pub fn new(pattern: impl Into<String>) -> Self {
        SearchQuery {
            pattern: pattern.into(),
            case_insensitive: false,
            path_glob: None,
            max_results: DEFAULT_MAX_RESULTS,
            context_lines: 0,
        }
    }

    /// Set case-insensitivity.
    pub fn case_insensitive(mut self, yes: bool) -> Self {
        self.case_insensitive = yes;
        self
    }

    /// Restrict to paths matching any of `globs` (gitignore-style, where
    /// `*` does not cross `/` and `**` does).
    pub fn path_globs<I, S>(mut self, globs: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = GlobSetBuilder::new();
        let mut count = 0;
        for g in globs {
            builder.add(Glob::new(g.as_ref())?);
            count += 1;
        }
        // An empty glob set matches nothing; no globs means no filter.
        self.path_glob = (count > 0).then(|| builder.build()).transpose()?;
        Ok(self)
    }

    /// Restrict to paths matching one glob.
    pub fn path_glob(self, glob: &str) -> Result<Self> {
        self.path_globs([glob])
    }

    /// Cap the number of hits.
    pub fn max_results(mut self, n: usize) -> Self {
        self.max_results = n;
        self
    }

    /// Lines of context around each hit.
    pub fn context_lines(mut self, n: usize) -> Self {
        self.context_lines = n;
        self
    }

    /// Compile the pattern as a multi-line byte regex.
    pub fn compile(&self) -> Result<Regex> {
        Ok(RegexBuilder::new(&self.pattern)
            .case_insensitive(self.case_insensitive)
            .multi_line(true)
            .build()?)
    }

    /// The trigram query to run against a folded index. The index folds
    /// case regardless of the query, so literals are always folded here.
    pub fn trigram_query(&self) -> Result<TrigramQuery> {
        required_trigrams_for_regex(&self.pattern, true)
    }

    /// Whether `path` passes the glob filter.
    pub fn path_allowed(&self, path: &str) -> bool {
        self.path_glob.as_ref().is_none_or(|g| g.is_match(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_filter_paths() {
        let q = SearchQuery::new("x").path_glob("**/*.rs").unwrap();
        assert!(q.path_allowed("src/lib.rs"));
        assert!(q.path_allowed("lib.rs"));
        assert!(!q.path_allowed("src/lib.py"));
        assert!(SearchQuery::new("x").path_allowed("anything"));
        assert!(SearchQuery::new("x").path_glob("[").is_err());
    }

    #[test]
    fn empty_glob_list_is_no_filter() {
        let q = SearchQuery::new("x")
            .path_globs(std::iter::empty::<&str>())
            .unwrap();
        assert!(q.path_allowed("src/lib.rs"));
    }

    #[test]
    fn compiles_with_flags() {
        let re = SearchQuery::new("hello")
            .case_insensitive(true)
            .compile()
            .unwrap();
        assert!(re.is_match(b"HELLO"));
        assert!(SearchQuery::new("(").compile().is_err());
    }
}
