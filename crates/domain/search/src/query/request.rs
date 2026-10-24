//! Search parameters.

use std::time::Duration;

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::bytes::{Regex, RegexBuilder};

use super::matcher::Matcher;
use crate::error::{Error, Result};
use crate::structural::StructuralPattern;
use crate::trigram::{required_trigrams_for_regex, TrigramQuery};

/// Default cap on returned hits.
pub const DEFAULT_MAX_RESULTS: usize = 100;
/// Default time a query may spend before it returns partial results.
pub const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(10);
/// Most memory a compiled pattern may use; larger patterns are refused.
const REGEX_SIZE_LIMIT: usize = 16 * 1024 * 1024;

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
    /// Match the pattern as a literal string, not a regex.
    pub literal: bool,
    /// Only files in these languages (as [`super::language_of`] names
    /// them, case-insensitive); empty is every language.
    pub languages: Vec<String>,
    /// Time after which the search stops and reports partial results.
    pub time_budget: Duration,
    /// The pattern is an ast-grep structural pattern in the one language
    /// `languages` names.
    pub structural: bool,
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
            literal: false,
            languages: Vec::new(),
            time_budget: DEFAULT_TIME_BUDGET,
            structural: false,
        }
    }

    /// Match structurally.
    pub fn structural(mut self, yes: bool) -> Self {
        self.structural = yes;
        self
    }

    /// A case-sensitive search for `word` as a whole word.
    pub fn whole_word(word: &str) -> Self {
        SearchQuery::new(format!(r"\b{}\b", regex::escape(word)))
    }

    /// Match literally.
    pub fn literal(mut self, yes: bool) -> Self {
        self.literal = yes;
        self
    }

    /// Restrict to languages.
    pub fn languages(mut self, langs: Vec<String>) -> Self {
        self.languages = langs;
        self
    }

    /// Set the time budget.
    pub fn time_budget(mut self, budget: Duration) -> Self {
        self.time_budget = budget;
        self
    }

    /// The regex syntax of the pattern.
    fn regex_source(&self) -> String {
        if self.literal {
            regex::escape(&self.pattern)
        } else {
            self.pattern.clone()
        }
    }

    /// Whether `path`'s language passes the language filter.
    pub fn language_allowed(&self, path: &str) -> bool {
        self.languages.is_empty() || {
            let lang = super::language_of(path);
            self.languages.iter().any(|l| l.eq_ignore_ascii_case(lang))
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
        Ok(RegexBuilder::new(&self.regex_source())
            .case_insensitive(self.case_insensitive)
            .multi_line(true)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .build()?)
    }

    /// The trigram query to run against a folded index. The index folds
    /// case regardless of the query, so literals are always folded here.
    pub fn trigram_query(&self) -> Result<TrigramQuery> {
        required_trigrams_for_regex(&self.regex_source(), true)
    }

    /// Compile the pattern: structurally for a structural query, else as
    /// [`SearchQuery::compile`] does.
    pub fn matcher(&self) -> Result<Matcher> {
        if !self.structural {
            return Ok(Matcher::Regex {
                regex: self.compile()?,
                source: self.regex_source(),
            });
        }
        match self.languages.as_slice() {
            [language] => Ok(Matcher::Structural(StructuralPattern::new(
                &self.pattern,
                language,
            )?)),
            _ => Err(Error::InvalidPattern(
                "structural search needs exactly one language".into(),
            )),
        }
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
