//! A repository's `.rill/review.toml`: which reviewers see which changes,
//! and which agent resolves conflicts.
//!
//! ```toml
//! [[reviewer]]
//! agent = "reviewer-security"
//! paths = ["services/**", "*.proto"]   # default: every path
//! exclude = ["**/testdata/**"]
//! min_risk = "normal"                  # default: trivial
//!
//! [resolver]
//! agent = "resolver"
//! paths = ["**"]
//! ```

use std::str::FromStr;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rill_types::{RepoPath, RiskClass};
use serde::Deserialize;

use crate::error::{Error, Result};

/// Where the routing lives on trunk.
pub const REVIEW_CONFIG: &str = ".rill/review.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    reviewer: Vec<RawRoute>,
    resolver: Option<RawRoute>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoute {
    agent: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    min_risk: Option<String>,
}

/// One `[[reviewer]]`: an agent and the changes it reviews.
#[derive(Debug, Clone)]
pub struct ReviewerRoute {
    pub agent: String,
    pub min_risk: RiskClass,
    paths: GlobSet,
    exclude: GlobSet,
}

/// The `[resolver]`: the agent that resolves conflicts on these paths.
#[derive(Debug, Clone)]
pub struct ResolverRoute {
    pub agent: String,
    paths: GlobSet,
    exclude: GlobSet,
}

/// A parsed `.rill/review.toml`.
#[derive(Debug, Clone, Default)]
pub struct ReviewConfig {
    pub reviewers: Vec<ReviewerRoute>,
    pub resolver: Option<ResolverRoute>,
}

fn globs(patterns: &[String], default_all: bool) -> Result<GlobSet> {
    let mut set = GlobSetBuilder::new();
    let all = ["**".to_string()];
    let patterns = if patterns.is_empty() && default_all {
        &all[..]
    } else {
        patterns
    };
    for p in patterns {
        let glob = GlobBuilder::new(p.trim_start_matches('/'))
            .literal_separator(true)
            .build()
            .map_err(|e| Error::InvalidGlob {
                glob: p.clone(),
                reason: e.to_string(),
            })?;
        set.add(glob);
    }
    set.build().map_err(|e| Error::InvalidGlob {
        glob: patterns.join(", "),
        reason: e.to_string(),
    })
}

fn agent_name(raw: &str) -> Result<String> {
    let name = raw.strip_prefix("agent:").unwrap_or(raw);
    if name.is_empty() || name.contains([':', '/', ' ']) {
        return Err(Error::PolicyParse(format!(
            "{REVIEW_CONFIG}: {raw:?} is not an agent name"
        )));
    }
    Ok(name.to_string())
}

fn matches(paths: &GlobSet, exclude: &GlobSet, path: &RepoPath) -> bool {
    paths.is_match(path.as_str()) && !exclude.is_match(path.as_str())
}

impl ReviewConfig {
    /// Parse and check a file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: RawConfig = toml::from_str(text)
            .map_err(|e| Error::PolicyParse(format!("{REVIEW_CONFIG}: {e}")))?;
        let mut reviewers = Vec::with_capacity(raw.reviewer.len());
        for r in raw.reviewer {
            let min_risk = match &r.min_risk {
                Some(s) => RiskClass::from_str(s).map_err(|_| {
                    Error::PolicyParse(format!("{REVIEW_CONFIG}: unknown risk {s:?}"))
                })?,
                None => RiskClass::Trivial,
            };
            let agent = agent_name(&r.agent)?;
            if reviewers.iter().any(|x: &ReviewerRoute| x.agent == agent) {
                return Err(Error::PolicyParse(format!(
                    "{REVIEW_CONFIG}: reviewer {agent} is listed twice"
                )));
            }
            reviewers.push(ReviewerRoute {
                agent,
                min_risk,
                paths: globs(&r.paths, true)?,
                exclude: globs(&r.exclude, false)?,
            });
        }
        let resolver = match raw.resolver {
            Some(r) if r.min_risk.is_some() => {
                return Err(Error::PolicyParse(format!(
                    "{REVIEW_CONFIG}: the resolver takes no min_risk"
                )))
            }
            Some(r) => Some(ResolverRoute {
                agent: agent_name(&r.agent)?,
                paths: globs(&r.paths, true)?,
                exclude: globs(&r.exclude, false)?,
            }),
            None => None,
        };
        Ok(ReviewConfig {
            reviewers,
            resolver,
        })
    }

    /// The reviewers a change at `risk` touching `paths` goes to, in file
    /// order: those at or below its risk with a path in their globs.
    pub fn route<'a>(
        &'a self,
        paths: &[&RepoPath],
        risk: RiskClass,
    ) -> impl Iterator<Item = &'a ReviewerRoute> + 'a {
        let paths: Vec<_> = paths.iter().map(|p| p.to_owned_path()).collect();
        self.reviewers.iter().filter(move |r| {
            risk >= r.min_risk
                && paths
                    .iter()
                    .any(|p| matches(&r.paths, &r.exclude, p.as_path()))
        })
    }

    /// The resolver for conflicts on `paths`, when every one is its to
    /// resolve.
    pub fn resolver_for(&self, paths: &[&RepoPath]) -> Option<&ResolverRoute> {
        self.resolver
            .as_ref()
            .filter(|r| !paths.is_empty() && paths.iter().all(|p| matches(&r.paths, &r.exclude, p)))
    }
}

#[cfg(test)]
mod tests {
    use rill_types::RepoPathBuf;

    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn routing_honours_paths_exclusions_and_risk() {
        let config = ReviewConfig::parse(
            r#"
            [[reviewer]]
            agent = "general"
            [[reviewer]]
            agent = "agent:security"
            paths = ["services/**"]
            exclude = ["**/testdata/**"]
            min_risk = "normal"
            [resolver]
            agent = "fixer"
            paths = ["services/**"]
            "#,
        )
        .unwrap();
        let names = |paths: &[RepoPathBuf], risk| {
            let refs: Vec<&RepoPath> = paths.iter().map(|p| p.as_path()).collect();
            config
                .route(&refs, risk)
                .map(|r| r.agent.clone())
                .collect::<Vec<_>>()
        };
        let svc = [p("services/a.rs")];
        assert_eq!(names(&svc, RiskClass::Normal), ["general", "security"]);
        assert_eq!(names(&svc, RiskClass::Low), ["general"]);
        assert_eq!(
            names(&[p("services/testdata/x")], RiskClass::Critical),
            ["general"]
        );
        assert!(config.resolver_for(&[svc[0].as_path()]).is_some());
        assert!(config
            .resolver_for(&[svc[0].as_path(), p("docs/x").as_path()])
            .is_none());
    }

    #[test]
    fn bad_files_are_refused() {
        for text in [
            "[[reviewer]]\nagent = \"a\"\nmin_risk = \"huge\"",
            "[[reviewer]]\nagent = \"user:ann\"",
            "[[reviewer]]\nagent = \"a\"\n[[reviewer]]\nagent = \"a\"",
            "[[reviewer]]\nagent = \"a\"\npaths = [\"[\"]",
            "[[reviewer]]\nagent = \"a\"\nmodel = \"x\"",
        ] {
            assert!(ReviewConfig::parse(text).is_err(), "{text}");
        }
    }
}
