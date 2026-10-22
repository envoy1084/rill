//! Whether a version's author may still write what it changes, asked by
//! the lander when it commits a landing: a revoke between commit and land
//! keeps the change off trunk.

use std::sync::Arc;

use rill_core::{DeriveManager, TreeOps};
use rill_policy::rebac::dir_object;
use rill_policy::{Authorizer, Relation};
use rill_types::identity::OrgRole;
use rill_types::{ChangeVersion, PrincipalKind, RepoPathBuf};

use crate::identity::IdentityView;
use crate::writes::{is_config, write_set};

/// The author's write rights over a repository's versions.
pub struct WriteRights {
    pub org: String,
    pub authorizer: Arc<Authorizer>,
    pub identity: Arc<IdentityView>,
    pub trees: TreeOps,
    pub derive: Arc<DeriveManager>,
}

impl WriteRights {
    /// Why `cv` must not land on its author's authority; `None` when it may.
    /// Lookup failures refuse.
    pub async fn refusal(&self, cv: &ChangeVersion) -> Option<String> {
        match self.check(cv).await {
            Ok(None) => None,
            Ok(Some(reason)) => Some(reason),
            Err(e) => Some(format!(
                "its author's write rights could not be checked: {e}"
            )),
        }
    }

    async fn check(&self, cv: &ChangeVersion) -> crate::error::Result<Option<String>> {
        let author = &cv.author;
        let (teams, scope, admin) = match author.kind {
            PrincipalKind::Human => match self.identity.user(&self.org, &author.name).await? {
                Some(user) => (
                    user.teams.clone(),
                    Vec::new(),
                    user.has_role(OrgRole::Admin),
                ),
                None => (Vec::new(), Vec::new(), false),
            },
            PrincipalKind::Agent => match self.identity.agent(&self.org, &author.name).await? {
                Some(agent) => (
                    Vec::new(),
                    agent
                        .paths
                        .iter()
                        .filter_map(|p| RepoPathBuf::new(p.as_str()).ok())
                        .collect(),
                    false,
                ),
                None => (Vec::new(), Vec::new(), false),
            },
            PrincipalKind::Service => (Vec::new(), Vec::new(), false),
        };
        let admin = admin || self.identity.is_configured_admin(author);
        let set = write_set(&self.trees, &self.derive, cv).await?;
        let snapshot = self.authorizer.snapshot().await?;
        let in_scope = |p: &RepoPathBuf| scope.is_empty() || scope.iter().any(|s| p.starts_with(s));
        let holds = |relation: Relation, p: &RepoPathBuf| {
            in_scope(p) && (admin || snapshot.check(author, &teams, relation, &dir_object(p)))
        };
        for path in &set.writes {
            let relation = if is_config(path) {
                Relation::Owner
            } else {
                Relation::Writer
            };
            if !holds(relation, path) {
                return Ok(Some(format!("its author may no longer write {path}")));
            }
        }
        for path in &set.copies_from {
            if !holds(Relation::Reader, path) {
                return Ok(Some(format!("its author may no longer read {path}")));
            }
        }
        Ok(None)
    }
}
