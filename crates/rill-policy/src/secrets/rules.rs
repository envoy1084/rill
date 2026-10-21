//! A repository's scanning rules, `.rill/secrets.toml` on trunk:
//!
//! ```toml
//! mode = "block"                 # or "warn", "quarantine"
//! allow_paths = ["tests/fixtures/**"]
//! allow_fingerprints = ["3f2a…"]  # from a finding, for a known fake
//!
//! [[pattern]]
//! name = "acme-deploy-key"
//! regex = 'acme_dk_[0-9a-f]{40}'
//! ```

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use serde::Deserialize;

use crate::error::{Error, Result};

/// Where the rules live.
pub const RULES_PATH: &str = ".rill/secrets.toml";

/// What a finding does to the version it is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    /// Refuse the version.
    #[default]
    Block,
    /// Store it, and audit the finding.
    Warn,
    /// Store it, but refuse to land it until an owner clears it.
    Quarantine,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    mode: ScanMode,
    #[serde(default)]
    allow_paths: Vec<String>,
    #[serde(default)]
    allow_fingerprints: Vec<String>,
    #[serde(default)]
    pattern: Vec<RawPattern>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPattern {
    name: String,
    regex: String,
}

/// The parsed rules; [`SecretRules::default`] when a repository has none.
#[derive(Debug, Clone, Default)]
pub struct SecretRules {
    pub mode: ScanMode,
    pub(super) allow_paths: Option<GlobSet>,
    pub(super) allow_fingerprints: Vec<String>,
    pub(super) patterns: Vec<(String, Regex)>,
}

impl SecretRules {
    /// Parse the file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = |why: String| Error::PolicyParse(format!("{RULES_PATH}: {why}"));
        let raw: Raw = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        let allow_paths = if raw.allow_paths.is_empty() {
            None
        } else {
            let mut set = GlobSetBuilder::new();
            for p in &raw.allow_paths {
                set.add(Glob::new(p).map_err(|e| bad(format!("allow_paths {p:?}: {e}")))?);
            }
            Some(set.build().map_err(|e| bad(e.to_string()))?)
        };
        let patterns = raw
            .pattern
            .into_iter()
            .map(|p| {
                let regex = regex::RegexBuilder::new(&p.regex)
                    .size_limit(1 << 20)
                    .build()
                    .map_err(|e| bad(format!("pattern {:?}: {e}", p.name)))?;
                Ok((p.name, regex))
            })
            .collect::<Result<_>>()?;
        Ok(SecretRules {
            mode: raw.mode,
            allow_paths,
            allow_fingerprints: raw.allow_fingerprints,
            patterns,
        })
    }

    /// Whether findings in `path` are allowed.
    pub fn path_allowed(&self, path: &str) -> bool {
        self.allow_paths
            .as_ref()
            .is_some_and(|set| set.is_match(path))
    }
}
