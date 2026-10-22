//! A repository's merge drivers, `.rill/merge.toml` at the head a change
//! lands on:
//!
//! ```toml
//! [[driver]]
//! paths = ["CHANGELOG.md", "registry/*.toml"]
//! merge = "union"            # both sides' added lines
//!
//! [[driver]]
//! paths = ["snapshots/**"]
//! merge = "generated"        # the head's version; regenerated later
//!
//! [[driver]]
//! name = "cargo"
//! paths = ["Cargo.lock"]
//! merge = "regenerate"       # the head's version, then the command
//! command = ["cargo", "update", "--workspace", "--offline"]
//! inputs = ["Cargo.lock", "**/Cargo.toml"]   # the whole tree when omitted
//! env = { CARGO_NET_OFFLINE = "true" }
//! platform = { OSFamily = "linux" }
//! timeout_secs = 300
//! risk = ["trivial", "low", "normal"]        # every class when omitted
//!
//! [ladder]
//! manual = ["auth/**"]       # conflicts here always go to the owner
//! ```
//!
//! `*` stays within a directory and `**` crosses directories. The first
//! driver whose paths match a file applies; a file no driver names falls
//! back to the root `.gitattributes` (`merge=union`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use std::time::Duration;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rill_types::RiskClass;
use serde::Deserialize;

use super::attributes::{GitAttributes, MergeAttribute};
use crate::error::{Error, Result};

/// Where the drivers are declared.
pub const MERGE_RULES_PATH: &str = ".rill/merge.toml";

/// Timeout of a regenerate command that declares none.
pub const DEFAULT_REGENERATE_TIMEOUT: Duration = Duration::from_secs(300);

/// How a driver resolves a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverKind {
    /// Keep the lines both sides added, the head's first.
    Union,
    /// Take the head's version; a later change regenerates it.
    Generated,
    /// Take the head's version, then rebuild the file by running a
    /// command on the merged tree.
    Regenerate(Regeneration),
}

/// A `regenerate` driver's command, run as a sandboxed action at the
/// root of the merged tree. It has no network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regeneration {
    pub command: Vec<String>,
    /// Globs of the files it reads; empty is the whole tree.
    pub inputs: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub platform: BTreeMap<String, String>,
    pub timeout: Duration,
}

/// One declared driver.
#[derive(Debug, Clone)]
pub struct Driver {
    pub name: String,
    pub kind: DriverKind,
    paths: GlobSet,
    /// Risk classes it resolves for; `None` is every class.
    risk: Option<BTreeSet<RiskClass>>,
}

/// The driver `.gitattributes` gives `merge=union` files.
static ATTRIBUTE_UNION: LazyLock<Driver> = LazyLock::new(|| Driver {
    name: String::new(),
    kind: DriverKind::Union,
    paths: GlobSet::empty(),
    risk: None,
});

/// The parsed drivers; [`MergeRules::default`] resolves nothing.
#[derive(Debug, Clone, Default)]
pub struct MergeRules {
    drivers: Vec<Driver>,
    manual: Option<GlobSet>,
    attributes: Option<GitAttributes>,
}

impl MergeRules {
    /// Parse the file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        let mut names = BTreeSet::new();
        let mut drivers = Vec::with_capacity(raw.driver.len());
        for (index, d) in raw.driver.into_iter().enumerate() {
            let name = d.name.clone().unwrap_or_else(|| match d.merge.as_str() {
                "regenerate" => format!("regenerate-{index}"),
                other => other.to_string(),
            });
            if d.paths.is_empty() {
                return Err(bad(format!("driver {name:?} names no paths")));
            }
            if d.merge == "regenerate" && !names.insert(name.clone()) {
                return Err(bad(format!("two regenerate drivers are named {name:?}")));
            }
            let kind = match d.merge.as_str() {
                "union" | "generated" if !d.command.is_empty() => {
                    return Err(bad(format!(
                        "driver {name:?}: only regenerate runs a command"
                    )))
                }
                "union" => DriverKind::Union,
                "generated" => DriverKind::Generated,
                "regenerate" if d.command.is_empty() => {
                    return Err(bad(format!("driver {name:?} has an empty command")))
                }
                "regenerate" => DriverKind::Regenerate(Regeneration {
                    command: d.command,
                    inputs: d.inputs,
                    env: d.env,
                    platform: d.platform,
                    timeout: d
                        .timeout_secs
                        .map(Duration::from_secs)
                        .unwrap_or(DEFAULT_REGENERATE_TIMEOUT),
                }),
                other => {
                    return Err(bad(format!(
                        "driver {name:?}: merge is union, generated or regenerate, not {other:?}"
                    )))
                }
            };
            let risk = d
                .risk
                .map(|classes| {
                    classes
                        .iter()
                        .map(|c| c.parse::<RiskClass>())
                        .collect::<std::result::Result<BTreeSet<_>, _>>()
                })
                .transpose()
                .map_err(|e| bad(format!("driver {name:?}: {e}")))?;
            drivers.push(Driver {
                paths: globs(&d.paths).map_err(|e| bad(format!("driver {name:?}: {e}")))?,
                name,
                kind,
                risk,
            });
        }
        let manual = match raw.ladder.manual.is_empty() {
            true => None,
            false => Some(globs(&raw.ladder.manual).map_err(|e| bad(format!("manual: {e}")))?),
        };
        Ok(MergeRules {
            drivers,
            manual,
            attributes: None,
        })
    }

    /// The same rules, falling back to a root `.gitattributes`.
    pub fn with_attributes(mut self, attributes: Option<GitAttributes>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Whether conflicts at `path` must go to the owner: declared manual,
    /// or `-merge` / `merge=binary` in `.gitattributes`.
    pub fn is_manual(&self, path: &str) -> bool {
        self.manual.as_ref().is_some_and(|set| set.is_match(path))
            || self
                .attributes
                .as_ref()
                .is_some_and(|a| a.merge(path) == MergeAttribute::Manual)
    }

    /// The driver resolving `path` for a change of `risk`, if any.
    pub fn driver_for(&self, path: &str, risk: RiskClass) -> Option<&Driver> {
        if let Some(driver) = self.drivers.iter().find(|d| d.paths.is_match(path)) {
            return driver
                .risk
                .as_ref()
                .is_none_or(|classes| classes.contains(&risk))
                .then_some(driver);
        }
        let union = self.attributes.as_ref()?.merge(path) == MergeAttribute::Union;
        union.then_some(&*ATTRIBUTE_UNION)
    }
}

impl Driver {
    /// The name recorded on what it resolves: its own, or the kind.
    pub fn label(&self) -> &str {
        match (self.name.is_empty(), &self.kind) {
            (false, _) => &self.name,
            (true, DriverKind::Union) => "gitattributes union",
            (true, _) => "driver",
        }
    }
}

fn globs(patterns: &[String]) -> std::result::Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for p in patterns {
        let glob = GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .map_err(|e| format!("{p:?}: {e}"))?;
        set.add(glob);
    }
    set.build().map_err(|e| e.to_string())
}

fn bad(why: String) -> Error {
    Error::Config(format!("{MERGE_RULES_PATH}: {why}"))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    driver: Vec<RawDriver>,
    #[serde(default)]
    ladder: RawLadder,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDriver {
    name: Option<String>,
    paths: Vec<String>,
    merge: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    platform: BTreeMap<String, String>,
    timeout_secs: Option<u64>,
    risk: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLadder {
    #[serde(default)]
    manual: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_matching_driver_applies_within_its_risk_classes() {
        let rules = MergeRules::parse(
            r#"
[[driver]]
paths = ["registry/*.toml"]
merge = "union"
risk = ["low"]

[[driver]]
name = "cargo"
paths = ["Cargo.lock", "registry/**"]
merge = "regenerate"
command = ["cargo", "update", "--offline"]

[ladder]
manual = ["auth/**"]
"#,
        )
        .unwrap();
        let union = rules.driver_for("registry/a.toml", RiskClass::Low).unwrap();
        assert_eq!(union.kind, DriverKind::Union);
        assert!(
            rules
                .driver_for("registry/a.toml", RiskClass::Critical)
                .is_none(),
            "the matching driver does not resolve this class, and later ones are not tried"
        );
        assert!(
            matches!(
                rules.driver_for("registry/x/b.toml", RiskClass::Low).map(|d| &d.kind),
                Some(DriverKind::Regenerate(r)) if r.timeout == DEFAULT_REGENERATE_TIMEOUT
            ),
            "* stays within a directory"
        );
        assert!(rules.driver_for("src/a.rs", RiskClass::Low).is_none());
        assert!(rules.is_manual("auth/keys.rs") && !rules.is_manual("src/a.rs"));
    }

    #[test]
    fn drivers_that_cannot_run_as_written_are_refused() {
        for text in [
            "[[driver]]\npaths = [\"a\"]\nmerge = \"ours\"\n",
            "[[driver]]\npaths = []\nmerge = \"union\"\n",
            "[[driver]]\npaths = [\"a\"]\nmerge = \"regenerate\"\n",
            "[[driver]]\npaths = [\"a\"]\nmerge = \"union\"\ncommand = [\"x\"]\n",
            "[[driver]]\npaths = [\"[\"]\nmerge = \"union\"\n",
            "[[driver]]\npaths = [\"a\"]\nmerge = \"union\"\nrisk = [\"spicy\"]\n",
            "[[driver]]\nname = \"x\"\npaths = [\"a\"]\nmerge = \"regenerate\"\ncommand = [\"x\"]\n\
             [[driver]]\nname = \"x\"\npaths = [\"b\"]\nmerge = \"regenerate\"\ncommand = [\"y\"]\n",
            "[ladder]\nagent = true\n",
        ] {
            assert!(MergeRules::parse(text).is_err(), "{text}");
        }
    }
}
