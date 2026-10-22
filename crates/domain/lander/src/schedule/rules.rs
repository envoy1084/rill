//! A repository's landing rules, `.rill/landing.toml` on trunk:
//!
//! ```toml
//! # Paths that always land one after another.
//! [[serialize]]
//! name = "schema"
//! paths = ["schema/**", "migrations/**"]
//!
//! [throttle]
//! conflict_rate = 0.5   # of rebase attempts in a domain
//! min_attempts = 10
//! window_secs = 300
//! ```

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::error::{Error, Result};

/// Where the rules live.
pub const RULES_PATH: &str = ".rill/landing.toml";

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    serialize: Vec<RawGroup>,
    #[serde(default)]
    throttle: Option<ThrottleRules>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGroup {
    name: String,
    paths: Vec<String>,
}

/// When a domain is throttled for swarm thrash.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThrottleRules {
    /// Share of rebase attempts that conflicted.
    pub conflict_rate: f64,
    /// Attempts in the window before the rate counts.
    pub min_attempts: u32,
    pub window_secs: u64,
}

impl Default for ThrottleRules {
    fn default() -> Self {
        ThrottleRules {
            conflict_rate: 0.5,
            min_attempts: 10,
            window_secs: 300,
        }
    }
}

/// The parsed rules; [`LandingRules::default`] when a repository has none.
#[derive(Debug, Clone, Default)]
pub struct LandingRules {
    groups: Vec<(String, GlobSet)>,
    pub throttle: ThrottleRules,
}

impl LandingRules {
    /// Parse the file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = |why: String| Error::Config(format!("{RULES_PATH}: {why}"));
        let raw: Raw = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        let mut groups = Vec::with_capacity(raw.serialize.len());
        for group in raw.serialize {
            let mut set = GlobSetBuilder::new();
            for p in &group.paths {
                set.add(Glob::new(p).map_err(|e| bad(format!("{}: {p:?}: {e}", group.name)))?);
            }
            groups.push((group.name, set.build().map_err(|e| bad(e.to_string()))?));
        }
        let throttle = raw.throttle.unwrap_or_default();
        if !(0.0..=1.0).contains(&throttle.conflict_rate) {
            return Err(bad("throttle.conflict_rate must be within 0..=1".into()));
        }
        Ok(LandingRules { groups, throttle })
    }

    /// Names of the declared groups `path` belongs to.
    pub fn groups_of<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.groups
            .iter()
            .filter(move |(_, set)| set.is_match(path))
            .map(|(name, _)| name.as_str())
    }
}
