//! What a capability token grants.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use rill_types::{RepoId, RepoPath, RepoPathBuf, Timestamp, WorkspaceId};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// An operation class a token may authorize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Verb {
    /// Read objects, history and review state.
    Read,
    /// Create and amend changes, snapshot workspaces.
    Write,
    /// Land changes on trunk.
    Land,
    /// Vote and comment.
    Review,
    /// Manage relations, policy and tokens.
    Admin,
    /// Run actions on the remote execution service.
    Execute,
}

impl Verb {
    /// All verbs.
    pub const ALL: [Verb; 6] = [
        Verb::Read,
        Verb::Write,
        Verb::Land,
        Verb::Review,
        Verb::Admin,
        Verb::Execute,
    ];

    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Read => "read",
            Verb::Write => "write",
            Verb::Land => "land",
            Verb::Review => "review",
            Verb::Admin => "admin",
            Verb::Execute => "execute",
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Verb {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Verb::ALL
            .into_iter()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| Error::Token(format!("unknown verb {s:?}")))
    }
}

/// The scope of a capability token: one repository, optionally one
/// workspace, a set of path prefixes, a set of verbs and an expiry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// Repository the token applies to.
    pub repo: RepoId,
    /// If set, the token is only valid for requests on this workspace.
    pub workspace: Option<WorkspaceId>,
    /// Path prefixes the token covers; the root covers everything. Empty
    /// means no path at all.
    pub paths: Vec<RepoPathBuf>,
    /// Verbs the token allows.
    pub verbs: Vec<Verb>,
    /// When the token stops being valid.
    pub expires_at: Timestamp,
}

impl Capability {
    /// A capability over the whole repository with the given verbs.
    pub fn repo_wide(
        repo: RepoId,
        verbs: impl IntoIterator<Item = Verb>,
        expires_at: Timestamp,
    ) -> Self {
        Capability {
            repo,
            workspace: None,
            paths: vec![RepoPathBuf::root()],
            verbs: verbs.into_iter().collect(),
            expires_at,
        }
    }

    /// Restrict to one workspace.
    pub fn with_workspace(mut self, workspace: WorkspaceId) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Restrict to path prefixes.
    pub fn with_paths(mut self, paths: impl IntoIterator<Item = RepoPathBuf>) -> Self {
        self.paths = paths.into_iter().collect();
        self
    }

    /// Distinct verbs, sorted.
    pub fn verb_set(&self) -> BTreeSet<Verb> {
        self.verbs.iter().copied().collect()
    }

    /// Whether `path` lies under one of the prefixes.
    pub fn covers_path(&self, path: &RepoPath) -> bool {
        self.paths.iter().any(|p| path.starts_with(p))
    }

    /// Whether the capability allows `verb`.
    pub fn allows(&self, verb: Verb) -> bool {
        self.verbs.contains(&verb)
    }

    /// Check that `self` grants no more than `parent`: same repository,
    /// same or newly-pinned workspace, verbs a subset, every path under one
    /// of the parent's, and an expiry no later than the parent's.
    pub fn check_narrower_than(&self, parent: &Capability) -> Result<()> {
        if self.repo != parent.repo {
            return Err(Error::Widening(format!(
                "repo {} is not parent repo {}",
                self.repo, parent.repo
            )));
        }
        match (parent.workspace, self.workspace) {
            (Some(p), Some(c)) if p != c => {
                return Err(Error::Widening(format!(
                    "workspace {c} is not parent workspace {p}"
                )))
            }
            (Some(p), None) => {
                return Err(Error::Widening(format!(
                    "parent is pinned to workspace {p}"
                )))
            }
            _ => {}
        }
        if let Some(v) = self.verb_set().difference(&parent.verb_set()).next() {
            return Err(Error::Widening(format!("verb {v} not held by parent")));
        }
        for p in &self.paths {
            if !parent.covers_path(p) {
                return Err(Error::Widening(format!(
                    "path {p:?} is not under a parent prefix"
                )));
            }
        }
        if self.expires_at > parent.expires_at {
            return Err(Error::Widening(format!(
                "expiry {} is after parent expiry {}",
                self.expires_at, parent.expires_at
            )));
        }
        Ok(())
    }

    /// Validate the capability itself: at least one verb and one path.
    pub fn validate(&self) -> Result<()> {
        if self.verbs.is_empty() {
            return Err(Error::Token("capability grants no verbs".into()));
        }
        if self.paths.is_empty() {
            return Err(Error::Token("capability covers no paths".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn parent() -> Capability {
        Capability::repo_wide(
            RepoId::derived("t", "r"),
            [Verb::Read, Verb::Write],
            Timestamp::from_millis(10_000),
        )
        .with_paths([p("src"), p("docs")])
    }

    #[test]
    fn narrowing_rules() {
        let parent = parent();
        let child = Capability {
            verbs: vec![Verb::Read],
            paths: vec![p("src/billing")],
            expires_at: Timestamp::from_millis(5_000),
            ..parent.clone()
        };
        child.check_narrower_than(&parent).unwrap();
        parent.check_narrower_than(&parent).unwrap();

        let wider_verb = Capability {
            verbs: vec![Verb::Land],
            ..child.clone()
        };
        assert!(matches!(
            wider_verb.check_narrower_than(&parent),
            Err(Error::Widening(_))
        ));
        let wider_path = Capability {
            paths: vec![p("srcx")],
            ..child.clone()
        };
        assert!(wider_path.check_narrower_than(&parent).is_err());
        let root_path = Capability {
            paths: vec![RepoPathBuf::root()],
            ..child.clone()
        };
        assert!(root_path.check_narrower_than(&parent).is_err());
        let later = Capability {
            expires_at: Timestamp::from_millis(20_000),
            ..child.clone()
        };
        assert!(later.check_narrower_than(&parent).is_err());
        let other_repo = Capability {
            repo: RepoId::derived("t", "other"),
            ..child.clone()
        };
        assert!(other_repo.check_narrower_than(&parent).is_err());

        let ws = WorkspaceId::derived("t", "w");
        let pinned = child.clone().with_workspace(ws);
        pinned.check_narrower_than(&parent).unwrap();
        assert!(child.check_narrower_than(&pinned).is_err());
        assert!(child
            .clone()
            .with_workspace(WorkspaceId::derived("t", "x"))
            .check_narrower_than(&pinned)
            .is_err());
    }

    #[test]
    fn verb_roundtrip() {
        for v in Verb::ALL {
            assert_eq!(v.as_str().parse::<Verb>().unwrap(), v);
        }
        assert!("fly".parse::<Verb>().is_err());
    }
}
