//! Relationship tuples and answers about them.

use rill_proto::rill::v1 as pb;
use rill_types::Principal;

/// A tuple: `subject` holds `relation` on `path` (the whole repository
/// when empty) of `repo`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Relation {
    /// `org/name`, or a name in the default org.
    pub repo: String,
    /// A directory; empty for the repository.
    pub path: String,
    /// `owner`, `writer`, `reviewer` or `reader`.
    pub relation: String,
    /// `user:<name>`, `agent:<name>`, `team:<name>`, `kind:<kind>`,
    /// `principal:pr_<hex>` or `*`.
    pub subject: String,
}

impl Relation {
    /// Build a tuple.
    pub fn new(
        repo: impl Into<String>,
        path: impl Into<String>,
        relation: impl Into<String>,
        subject: impl Into<String>,
    ) -> Self {
        Relation {
            repo: repo.into(),
            path: path.into(),
            relation: relation.into(),
            subject: subject.into(),
        }
    }

    /// Wire encoding.
    pub fn to_proto(&self) -> pb::Relation {
        pb::Relation {
            repo: self.repo.clone(),
            path: self.path.clone(),
            relation: self.relation.clone(),
            subject: self.subject.clone(),
        }
    }
}

impl From<pb::Relation> for Relation {
    fn from(r: pb::Relation) -> Self {
        Relation {
            repo: r.repo,
            path: r.path,
            relation: r.relation,
            subject: r.subject,
        }
    }
}

/// Which tuples of a repository to list.
#[derive(Debug, Clone, Default)]
pub struct RelationFilter {
    /// Only tuples on this directory.
    pub path: Option<String>,
    /// Only tuples granted to this subject.
    pub subject: Option<String>,
    /// Only this relation.
    pub relation: Option<String>,
}

/// Whether someone holds a relation, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessCheck {
    /// Whether the relation is held.
    pub allowed: bool,
    /// The granting tuples, nearest directory first.
    pub because: Vec<Relation>,
    /// Allowed as an administrator of the repository's organization.
    pub org_admin: bool,
}

/// A granting tuple with the members of its team subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedRelation {
    /// The tuple.
    pub relation: Relation,
    /// The org's active users in its team, for a team subject.
    pub members: Vec<Principal>,
}
