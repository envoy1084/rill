//! `.rill/owners.toml`:
//!
//! ```toml
//! [[rule]]
//! path = "src/billing"           # a directory; "" is the repository
//! owners = ["user:ann", "team:billing", "agent:billing-bot"]
//! ```
//!
//! The rule with the longest path covering a file decides its owners;
//! rules do not add up.

use rill_types::{Principal, PrincipalKind, RepoPath, RepoPathBuf};
use serde::Deserialize;

use crate::error::{Error, Result};

/// Where the file lives.
pub const OWNERS_PATH: &str = ".rill/owners.toml";

/// One directory's owners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRule {
    pub path: RepoPathBuf,
    /// `user:<name>`, `agent:<name>` or `team:<name>`.
    pub owners: Vec<String>,
}

/// The parsed file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnersFile {
    rules: Vec<OwnerRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    rule: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    path: String,
    owners: Vec<String>,
}

impl OwnersFile {
    /// Parse the file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = |why: String| Error::PolicyParse(format!("{OWNERS_PATH}: {why}"));
        let raw: Raw = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        let mut rules = Vec::with_capacity(raw.rule.len());
        for r in raw.rule {
            let path = RepoPathBuf::new(r.path.trim_matches('/'))
                .map_err(|e| bad(format!("path {:?}: {e}", r.path)))?;
            for owner in &r.owners {
                let valid = ["user:", "agent:", "team:"]
                    .iter()
                    .any(|p| owner.strip_prefix(p).is_some_and(|n| !n.is_empty()));
                if !valid {
                    return Err(bad(format!(
                        "owner {owner:?} must be user:<name>, agent:<name> or team:<name>"
                    )));
                }
            }
            rules.push(OwnerRule {
                path,
                owners: r.owners,
            });
        }
        Ok(OwnersFile { rules })
    }

    /// The rule deciding `path`'s owners, if any covers it.
    pub fn rule_for(&self, path: &RepoPath) -> Option<&OwnerRule> {
        self.rules
            .iter()
            .filter(|r| path.starts_with(&r.path))
            .max_by_key(|r| r.path.as_str().len())
    }

    /// Whether `principal` (in `teams`) is among `rule`'s owners.
    pub fn is_owner(rule: &OwnerRule, principal: &Principal, teams: &[String]) -> bool {
        rule.owners.iter().any(|owner| match owner.split_once(':') {
            Some(("user", name)) => {
                principal.kind == PrincipalKind::Human && principal.name == name
            }
            Some(("agent", name)) => {
                principal.kind == PrincipalKind::Agent && principal.name == name
            }
            Some(("team", name)) => teams.iter().any(|t| t == name),
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    const FILE: &str = r#"
        [[rule]]
        path = ""
        owners = ["team:platform"]

        [[rule]]
        path = "src/billing"
        owners = ["user:ann", "agent:billing-bot"]
    "#;

    #[test]
    fn the_nearest_rule_decides_and_rules_do_not_add_up() {
        let file = OwnersFile::parse(FILE).unwrap();
        let billing = file.rule_for(&p("src/billing/invoice.rs")).unwrap();
        assert_eq!(billing.path, p("src/billing"));
        let platform = vec!["platform".to_string()];
        let ann = Principal::human("ann");
        assert!(OwnersFile::is_owner(billing, &ann, &[]));
        assert!(OwnersFile::is_owner(
            billing,
            &Principal::agent("billing-bot"),
            &[]
        ));
        assert!(
            !OwnersFile::is_owner(billing, &Principal::human("pat"), &platform),
            "the repository rule does not reach a directory with its own"
        );
        assert!(
            !OwnersFile::is_owner(billing, &Principal::agent("ann"), &[]),
            "kinds differ"
        );
        let other = file.rule_for(&p("docs/x.md")).unwrap();
        assert!(OwnersFile::is_owner(
            other,
            &Principal::human("pat"),
            &platform
        ));
        assert!(
            file.rule_for(&p("src/billingx/a")).unwrap().path.is_root(),
            "prefixes are whole components"
        );
    }

    #[test]
    fn malformed_files_are_refused() {
        assert!(OwnersFile::parse("[[rule]]\npath = \"a\"\nowners = [\"ann\"]").is_err());
        assert!(OwnersFile::parse("[[rule]]\npath = \"../x\"\nowners = []").is_err());
        assert!(OwnersFile::parse("[[rules]]").is_err());
        assert_eq!(OwnersFile::parse("").unwrap(), OwnersFile::default());
    }
}
