//! Which files of a tree an action reads.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rill_types::{ObjectHash, ObjectKind, RepoPath};

use crate::error::{Error, Result};

/// A set of input globs, or the whole tree. `*` stays within one path
/// component and `**` crosses them, so `src/**` is everything under
/// `src` and `*.toml` only the files at the root.
#[derive(Debug, Clone)]
pub struct InputFilter {
    globs: Option<GlobSet>,
    /// Where each pattern can match; a directory none reaches holds no
    /// match.
    reaches: Vec<Reach>,
    fingerprint: ObjectHash,
}

impl InputFilter {
    /// Every file.
    pub fn all() -> Self {
        InputFilter {
            globs: None,
            reaches: vec![Reach {
                dir: String::new(),
                deeper: true,
            }],
            fingerprint: ObjectHash::digest(ObjectKind::Derived, b"inputs:all"),
        }
    }

    /// The files matching any of `patterns`; no patterns means every file.
    pub fn new(patterns: &[String]) -> Result<Self> {
        if patterns.is_empty() {
            return Ok(Self::all());
        }
        let mut sorted: Vec<&str> = patterns.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let mut builder = GlobSetBuilder::new();
        let mut reaches = Vec::with_capacity(sorted.len());
        for pattern in &sorted {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|e| Error::Config {
                    path: crate::verify::PLAN_PATH.into(),
                    reason: format!("input {pattern:?}: {e}"),
                })?;
            builder.add(glob);
            reaches.push(Reach::of(pattern));
        }
        let globs = builder.build().map_err(|e| Error::Config {
            path: crate::verify::PLAN_PATH.into(),
            reason: e.to_string(),
        })?;
        Ok(InputFilter {
            globs: Some(globs),
            reaches,
            fingerprint: ObjectHash::digest(
                ObjectKind::Derived,
                format!("inputs:{}", sorted.join("\n")).as_bytes(),
            ),
        })
    }

    /// Whether the file at `path` is an input.
    pub fn matches(&self, path: &RepoPath) -> bool {
        match &self.globs {
            None => true,
            Some(globs) => globs.is_match(path.as_str()),
        }
    }

    /// Whether the directory at `dir` may hold an input.
    pub fn may_enter(&self, dir: &RepoPath) -> bool {
        let dir = dir.as_str();
        self.reaches
            .iter()
            .any(|r| within(&r.dir, dir) || (r.deeper && within(dir, &r.dir)))
    }

    /// Identifies the filter in derived keys.
    pub fn fingerprint(&self) -> ObjectHash {
        self.fingerprint
    }
}

/// The directories a pattern can match in.
#[derive(Debug, Clone)]
struct Reach {
    /// The literal directory before the first wildcard.
    dir: String,
    /// Whether matches may lie below `dir` rather than directly in it.
    deeper: bool,
}

impl Reach {
    fn of(pattern: &str) -> Self {
        let literal = pattern
            .find(['*', '?', '[', '{', '\\'])
            .map_or(pattern, |at| &pattern[..at]);
        let dir = literal
            .rfind('/')
            .map_or_else(String::new, |at| literal[..at].to_string());
        let rest = &pattern[if dir.is_empty() { 0 } else { dir.len() + 1 }..];
        Reach {
            deeper: rest.contains('/') || rest.contains("**"),
            dir,
        }
    }
}

/// `path` is `ancestor` or below it.
fn within(path: &str, ancestor: &str) -> bool {
    ancestor.is_empty()
        || path == ancestor
        || (path.len() > ancestor.len()
            && path.starts_with(ancestor)
            && path.as_bytes()[ancestor.len()] == b'/')
}

#[cfg(test)]
mod tests {
    use rill_types::RepoPathBuf;

    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn filter(patterns: &[&str]) -> InputFilter {
        InputFilter::new(&patterns.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn single_star_stays_in_its_directory() {
        let f = filter(&["*.toml", "src/**"]);
        assert!(f.matches(&p("Cargo.toml")));
        assert!(!f.matches(&p("crates/x/Cargo.toml")));
        assert!(f.matches(&p("src/a/b.rs")));
        assert!(!f.matches(&p("docs/a.md")));
    }

    #[test]
    fn directories_outside_every_pattern_are_not_entered() {
        let f = filter(&["crates/core/**/*.rs", "Cargo.lock"]);
        assert!(f.may_enter(&p("crates")));
        assert!(f.may_enter(&p("crates/core/src")));
        assert!(!f.may_enter(&p("crates/cor")));
        assert!(!f.may_enter(&p("docs")));
        assert!(!filter(&["*.toml"]).may_enter(&p("crates")));
        let anywhere = filter(&["**/*.rs"]);
        assert!(anywhere.may_enter(&p("docs/deep")));
    }

    #[test]
    fn pattern_order_does_not_change_the_fingerprint() {
        assert_eq!(
            filter(&["a/**", "b"]).fingerprint(),
            filter(&["b", "a/**", "b"]).fingerprint()
        );
        assert_ne!(
            filter(&["a/**"]).fingerprint(),
            InputFilter::all().fingerprint()
        );
    }
}
