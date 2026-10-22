//! The vocabulary of relationship tuples: `(object, relation, subject)`.

use std::fmt;
use std::str::FromStr;

use rill_types::{Principal, PrincipalId, PrincipalKind, RepoPath, RepoPathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A relation a subject can hold on an object. Each implies the ones
/// after it: `owner ⊃ writer ⊃ reviewer ⊃ reader`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Relation {
    /// Full control: relations, policy, token issuance.
    Owner,
    /// May create changes under the object and land them.
    Writer,
    /// May vote and comment on changes under the object.
    Reviewer,
    /// May read the object's content and history.
    Reader,
}

impl Relation {
    /// All relations, strongest first.
    pub const ALL: [Relation; 4] = [
        Relation::Owner,
        Relation::Writer,
        Relation::Reviewer,
        Relation::Reader,
    ];

    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Owner => "owner",
            Relation::Writer => "writer",
            Relation::Reviewer => "reviewer",
            Relation::Reader => "reader",
        }
    }

    /// The relations whose tuples satisfy a check for `self`: itself and
    /// every stronger one.
    pub fn satisfied_by(self) -> &'static [Relation] {
        let at = Relation::ALL
            .iter()
            .position(|r| *r == self)
            .unwrap_or(Relation::ALL.len() - 1);
        &Relation::ALL[..=at]
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Relation {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Relation::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| Error::PolicyEvaluation(format!("unknown relation {s:?}")))
    }
}

/// Who a tuple grants to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subject {
    /// One principal, by id.
    Principal(PrincipalId),
    /// Every member of a named team.
    Team(String),
    /// Every principal of a kind.
    Kind(PrincipalKind),
    /// Everyone.
    Any,
}

impl Subject {
    /// Subject for a specific principal.
    pub fn principal(p: &Principal) -> Self {
        Subject::Principal(p.id)
    }

    /// Whether this subject covers `principal`, given the teams it belongs to.
    pub fn matches(&self, principal: &Principal, teams: &[String]) -> bool {
        match self {
            Subject::Principal(id) => *id == principal.id,
            Subject::Team(name) => teams.iter().any(|t| t == name),
            Subject::Kind(kind) => *kind == principal.kind,
            Subject::Any => true,
        }
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Principal(id) => write!(f, "principal:{id}"),
            Subject::Team(name) => write!(f, "team:{name}"),
            Subject::Kind(kind) => write!(f, "kind:{}", kind.as_str()),
            Subject::Any => f.write_str("*"),
        }
    }
}

impl FromStr for Subject {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = |why: &str| Error::PolicyEvaluation(format!("invalid subject {s:?}: {why}"));
        if s == "*" {
            return Ok(Subject::Any);
        }
        let (prefix, rest) = s.split_once(':').ok_or_else(|| bad("missing prefix"))?;
        match prefix {
            "principal" => rest
                .parse()
                .map(Subject::Principal)
                .map_err(|e| bad(&e.to_string())),
            // Team names must not contain '/' because tuples are stored as
            // '/'-joined keys and parsed back by splitting on it.
            "team" if !rest.is_empty() && !rest.contains('/') => Ok(Subject::Team(rest.into())),
            "team" => Err(bad("team name must be non-empty and contain no '/'")),
            "kind" => match rest {
                "human" => Ok(Subject::Kind(PrincipalKind::Human)),
                "agent" => Ok(Subject::Kind(PrincipalKind::Agent)),
                "service" => Ok(Subject::Kind(PrincipalKind::Service)),
                _ => Err(bad("unknown principal kind")),
            },
            _ => Err(bad("unknown prefix")),
        }
    }
}

/// Object id of the repository as a whole.
pub const REPO_OBJECT: &str = "repo";

const DIR_PREFIX: &str = "dir:/";

/// Object id for a directory: `dir:/<path>`; the root directory is the
/// repository itself.
pub fn dir_object(path: &RepoPath) -> String {
    if path.is_root() {
        REPO_OBJECT.to_string()
    } else {
        format!("{DIR_PREFIX}{}", path.as_str())
    }
}

/// The directory a `dir:` object id names, if it is one.
pub fn dir_of_object(object: &str) -> Option<RepoPathBuf> {
    if object == REPO_OBJECT {
        return Some(RepoPathBuf::root());
    }
    let path = object.strip_prefix(DIR_PREFIX)?;
    RepoPathBuf::new(path).ok()
}

/// The chain of objects whose tuples apply to `object`, nearest first:
/// the object itself, each ancestor directory, then the repository.
/// Objects that are not directories have no ancestors.
pub fn inheritance_chain(object: &str) -> Vec<String> {
    let mut chain = vec![object.to_string()];
    let Some(dir) = dir_of_object(object) else {
        return chain;
    };
    let mut current: &RepoPath = &dir;
    while !current.is_root() {
        current = current.parent();
        chain.push(dir_object(current));
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_roundtrip_and_rewrites() {
        for r in Relation::ALL {
            assert_eq!(r.as_str().parse::<Relation>().unwrap(), r);
            assert!(r.satisfied_by().contains(&Relation::Owner));
            assert!(r.satisfied_by().contains(&r));
        }
        assert!("boss".parse::<Relation>().is_err());
        assert!("contributor".parse::<Relation>().is_err());
        assert_eq!(
            Relation::Reviewer.satisfied_by(),
            [Relation::Owner, Relation::Writer, Relation::Reviewer]
        );
        assert!(!Relation::Writer.satisfied_by().contains(&Relation::Reader));
    }

    #[test]
    fn subject_roundtrip() {
        let alice = Principal::human("alice");
        for s in [
            Subject::principal(&alice),
            Subject::Team("core".into()),
            Subject::Kind(PrincipalKind::Agent),
            Subject::Any,
        ] {
            assert_eq!(s.to_string().parse::<Subject>().unwrap(), s);
        }
        assert!("team:".parse::<Subject>().is_err());
        assert!("team:a/b".parse::<Subject>().is_err());
        assert!("kind:robot".parse::<Subject>().is_err());
        assert!("alice".parse::<Subject>().is_err());
    }

    #[test]
    fn subject_matching() {
        let alice = Principal::human("alice");
        let bot = Principal::agent("bot");
        let teams = vec!["core".to_string()];
        assert!(Subject::principal(&alice).matches(&alice, &[]));
        assert!(!Subject::principal(&alice).matches(&bot, &[]));
        assert!(Subject::Team("core".into()).matches(&alice, &teams));
        assert!(!Subject::Team("core".into()).matches(&alice, &[]));
        assert!(Subject::Kind(PrincipalKind::Agent).matches(&bot, &[]));
        assert!(!Subject::Kind(PrincipalKind::Agent).matches(&alice, &[]));
        assert!(Subject::Any.matches(&bot, &[]));
    }

    #[test]
    fn chains() {
        let p = RepoPathBuf::new("src/billing/x").unwrap();
        assert_eq!(
            inheritance_chain(&dir_object(&p)),
            vec!["dir:/src/billing/x", "dir:/src/billing", "dir:/src", "repo"]
        );
        assert_eq!(inheritance_chain("repo"), vec!["repo"]);
        assert_eq!(inheritance_chain("intent:abc"), vec!["intent:abc"]);
        assert_eq!(dir_object(&RepoPathBuf::root()), "repo");
        assert_eq!(
            dir_of_object("dir:/src").unwrap(),
            RepoPathBuf::new("src").unwrap()
        );
    }
}
