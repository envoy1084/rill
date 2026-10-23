//! One directory's `OWNERS.toml`:
//!
//! ```toml
//! owners = ["team:gateway"]          # this directory and below
//! inherit = true                     # also require the parent's owners
//! [[rule]]
//! paths = ["auth/**", "*.proto"]     # globs relative to this directory
//! owners = ["team:security", "user:alice"]
//! required = 2                       # approvals needed from this set
//! sensitive = true                   # raises the change's risk
//! sla = "8h"                         # then escalate
//! escalate_to = "team:security-oncall"
//! [[delegate]]
//! from = "user:alice"
//! to = "user:bob"
//! until = "2026-11-01"
//! ```
//!
//! Rules whose globs match a file each make an owner set; when none
//! matches, the directory's `owners` are the set. The nearest directory
//! with a file decides, unless it inherits its parent's sets too.

use std::time::Duration;

use globset::{GlobBuilder, GlobMatcher};
use rill_types::{RepoPath, Timestamp};
use serde::Deserialize;

use crate::error::{Error, Result};

/// The file name looked for in every directory.
pub const OWNERS_FILE: &str = "OWNERS.toml";

/// One `[[rule]]`.
#[derive(Debug, Clone)]
pub struct OwnerRule {
    /// The globs as written.
    pub paths: Vec<String>,
    pub owners: Vec<String>,
    /// Approvals needed from `owners`.
    pub required: u32,
    pub sensitive: bool,
    /// How long a required review may wait before escalating.
    pub sla: Option<Duration>,
    /// Who an overdue review escalates to.
    pub escalate_to: Option<String>,
    matchers: Vec<GlobMatcher>,
}

impl OwnerRule {
    /// Whether the rule covers `rel`, a path relative to the file's
    /// directory.
    pub fn matches(&self, rel: &str) -> bool {
        self.matchers.iter().any(|m| m.is_match(rel))
    }
}

/// One `[[delegate]]`: `from`'s approvals may come from `to` until a time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct Delegation {
    pub from: String,
    pub to: String,
    pub until: Timestamp,
}

/// A parsed `OWNERS.toml`.
#[derive(Debug, Clone, Default)]
pub struct OwnersFile {
    /// The directory's default owners.
    pub owners: Vec<String>,
    /// Also require the nearest parent file's owner sets.
    pub inherit: bool,
    pub rules: Vec<OwnerRule>,
    pub delegates: Vec<Delegation>,
    /// Escalation contact for the directory's default set.
    pub escalate_to: Option<String>,
    /// SLA of the directory's default set.
    pub sla: Option<Duration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    owners: Vec<String>,
    #[serde(default)]
    inherit: bool,
    #[serde(default)]
    sla: Option<String>,
    #[serde(default)]
    escalate_to: Option<String>,
    #[serde(default)]
    rule: Vec<RawRule>,
    #[serde(default)]
    delegate: Vec<RawDelegate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    paths: Vec<String>,
    owners: Vec<String>,
    #[serde(default)]
    required: Option<u32>,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    sla: Option<String>,
    #[serde(default)]
    escalate_to: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDelegate {
    from: String,
    to: String,
    until: String,
}

impl OwnersFile {
    /// Parse and check a file's text. Everything the lint refuses is an
    /// error here, so a file that parses is one the server can use.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).map_err(|e| bad(e.to_string()))?;
        for owner in raw.owners.iter() {
            check_owner(owner)?;
        }
        if raw.owners.is_empty() && !raw.inherit {
            return Err(bad(
                "`owners` is empty and `inherit` is false, which leaves the directory without owners"
                    .into(),
            ));
        }
        let mut rules = Vec::with_capacity(raw.rule.len());
        for r in raw.rule {
            if r.paths.is_empty() {
                return Err(bad("a rule needs at least one path".into()));
            }
            if r.owners.is_empty() {
                return Err(bad(format!("rule {:?} has no owners", r.paths)));
            }
            for owner in &r.owners {
                check_owner(owner)?;
            }
            let required = r.required.unwrap_or(1);
            if required == 0 {
                return Err(bad(format!(
                    "rule {:?}: `required` must be at least 1",
                    r.paths
                )));
            }
            let matchers = r
                .paths
                .iter()
                .map(|g| compile(g))
                .collect::<Result<Vec<_>>>()?;
            if let Some(to) = &r.escalate_to {
                check_owner(to)?;
            }
            rules.push(OwnerRule {
                paths: r.paths,
                owners: r.owners,
                required,
                sensitive: r.sensitive,
                sla: r.sla.as_deref().map(parse_sla).transpose()?,
                escalate_to: r.escalate_to,
                matchers,
            });
        }
        let mut delegates = Vec::with_capacity(raw.delegate.len());
        for d in raw.delegate {
            check_owner(&d.from)?;
            check_owner(&d.to)?;
            if d.from == d.to {
                return Err(bad(format!("{} delegates to themselves", d.from)));
            }
            delegates.push(Delegation {
                until: parse_until(&d.until)?,
                from: d.from,
                to: d.to,
            });
        }
        if let Some(to) = &raw.escalate_to {
            check_owner(to)?;
        }
        Ok(OwnersFile {
            owners: raw.owners,
            inherit: raw.inherit,
            rules,
            delegates,
            escalate_to: raw.escalate_to,
            sla: raw.sla.as_deref().map(parse_sla).transpose()?,
        })
    }

    /// The rules matching `rel`, a path relative to this file's directory.
    pub fn rules_for<'a>(&'a self, rel: &'a str) -> impl Iterator<Item = (usize, &'a OwnerRule)> {
        self.rules
            .iter()
            .enumerate()
            .filter(move |(_, r)| r.matches(rel))
    }
}

fn bad(why: String) -> Error {
    Error::PolicyParse(format!("{OWNERS_FILE}: {why}"))
}

/// `user:<name>`, `agent:<name>` or `team:<name>`.
fn check_owner(owner: &str) -> Result<()> {
    let valid = ["user:", "agent:", "team:"]
        .iter()
        .any(|p| owner.strip_prefix(p).is_some_and(|n| !n.is_empty()));
    if valid {
        Ok(())
    } else {
        Err(bad(format!(
            "owner {owner:?} must be user:<name>, agent:<name> or team:<name>"
        )))
    }
}

fn compile(glob: &str) -> Result<GlobMatcher> {
    GlobBuilder::new(glob.trim_start_matches('/'))
        .literal_separator(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|e| Error::InvalidGlob {
            glob: glob.to_string(),
            reason: e.to_string(),
        })
}

/// `90s`, `30m`, `8h` or `2d`.
pub fn parse_sla(text: &str) -> Result<Duration> {
    let text = text.trim();
    let (n, unit) = text.split_at(text.len().saturating_sub(1));
    let n: u64 = n
        .parse()
        .map_err(|_| bad(format!("sla {text:?} must be like 30m, 8h or 2d")))?;
    let secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad(format!("sla {text:?} must end in s, m, h or d"))),
    };
    if n == 0 {
        return Err(bad(format!("sla {text:?} must be positive")));
    }
    Ok(Duration::from_secs(n * secs))
}

/// `2026-11-01` (through the end of that day, UTC) or an RFC 3339 time.
fn parse_until(text: &str) -> Result<Timestamp> {
    let full = if text.len() == 10 {
        format!("{text}T23:59:59Z")
    } else {
        text.to_string()
    };
    Timestamp::parse_rfc3339(&full)
        .ok_or_else(|| bad(format!("delegation `until` {text:?} is not a date")))
}

/// `path` relative to `dir`, or `None` when it is not under it.
pub fn relative<'a>(dir: &RepoPath, path: &'a RepoPath) -> Option<&'a str> {
    if dir.is_root() {
        return Some(path.as_str());
    }
    path.strip_prefix(dir).map(RepoPath::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
        owners = ["team:gateway"]
        sla = "1d"
        [[rule]]
        paths = ["auth/**", "*.proto"]
        owners = ["team:security", "user:alice"]
        required = 2
        sensitive = true
        sla = "8h"
        escalate_to = "team:security-oncall"
        [[delegate]]
        from = "user:alice"
        to = "user:bob"
        until = "2026-11-01"
    "#;

    #[test]
    fn a_full_file_parses() {
        let f = OwnersFile::parse(FILE).unwrap();
        assert_eq!(f.owners, ["team:gateway"]);
        assert_eq!(f.sla, Some(Duration::from_secs(86_400)));
        let rule = &f.rules[0];
        assert_eq!(rule.required, 2);
        assert!(rule.sensitive && rule.matches("auth/login.rs") && rule.matches("api.proto"));
        assert!(!rule.matches("src/api.proto"), "`*` stops at a separator");
        assert_eq!(rule.sla, Some(Duration::from_secs(8 * 3600)));
        assert_eq!(
            f.delegates[0].until,
            Timestamp::parse_rfc3339("2026-11-01T23:59:59Z").unwrap()
        );
    }

    #[test]
    fn the_lint_refuses_what_would_leave_paths_ownerless_or_ambiguous() {
        for bad in [
            "owners = []",
            "owners = [\"alice\"]",
            "owners = [\"team:a\"]\n[[rule]]\npaths = []\nowners = [\"team:b\"]",
            "owners = [\"team:a\"]\n[[rule]]\npaths = [\"x\"]\nowners = []",
            "owners = [\"team:a\"]\n[[rule]]\npaths = [\"x\"]\nowners = [\"team:b\"]\nrequired = 0",
            "owners = [\"team:a\"]\n[[rule]]\npaths = [\"[\"]\nowners = [\"team:b\"]",
            "owners = [\"team:a\"]\nsla = \"8x\"",
            "owners = [\"team:a\"]\n[[delegate]]\nfrom = \"user:a\"\nto = \"user:a\"\nuntil = \"2026-01-01\"",
            "owners = [\"team:a\"]\n[[delegate]]\nfrom = \"user:a\"\nto = \"user:b\"\nuntil = \"soon\"",
            "owners = [\"team:a\"]\nextra = 1",
        ] {
            assert!(OwnersFile::parse(bad).is_err(), "accepted: {bad}");
        }
        assert!(OwnersFile::parse("inherit = true").is_ok());
    }

    #[test]
    fn paths_are_relative_to_the_owning_directory() {
        let dir = rill_types::RepoPathBuf::new("services/gateway").unwrap();
        let path = rill_types::RepoPathBuf::new("services/gateway/auth/x.rs").unwrap();
        assert_eq!(relative(&dir, &path), Some("auth/x.rs"));
        let other = rill_types::RepoPathBuf::new("services/gatewayx/a").unwrap();
        assert_eq!(relative(&dir, &other), None);
        let root = rill_types::RepoPathBuf::root();
        assert_eq!(relative(&root, &path), Some(path.as_str()));
    }
}
