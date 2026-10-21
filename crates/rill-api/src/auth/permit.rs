//! The permission check every handler runs before touching a repository.
//!
//! Two layers must both agree:
//!
//! 1. the caller's capability token, if it has one, must allow the verb on
//!    the path (`AuthContext::capability_allows`); a token is an
//!    attenuation of whatever the principal could otherwise do, so this is
//!    enforced even in dev mode;
//! 2. the ReBAC relations stored on the repository must grant the relation
//!    on the path's directory or an ancestor (`Authorizer::check_path`).
//!
//! The second layer is skipped in dev mode and for a repository that has
//! no relation tuples at all. The latter is the bootstrap rule: a fresh
//! repository is open until someone grants the first relation, at which
//! point only granted principals get in. `CreateRepo` grants `owner` to
//! the creator, so repositories made through the API are never in the
//! open state unless the creator removes themselves.

use rill_policy::{AuthContext, PathFilter, Relation, Verb};
use rill_types::{PrincipalKind, RepoPath, RepoPathBuf};

use crate::error::{Error, Result};
use crate::state::RepoContext;

/// Permission checks for one caller on one repository.
pub struct Permit<'a> {
    ctx: &'a AuthContext,
    repo: &'a RepoContext,
    dev_mode: bool,
}

impl<'a> Permit<'a> {
    /// Checks for `ctx` on `repo`.
    pub fn new(ctx: &'a AuthContext, repo: &'a RepoContext, dev_mode: bool) -> Self {
        Permit {
            ctx,
            repo,
            dev_mode,
        }
    }

    /// The caller.
    pub fn caller(&self) -> &'a AuthContext {
        self.ctx
    }

    /// Read access to `path` (or the whole repository for the root).
    pub async fn read(&self, path: &RepoPath) -> Result<()> {
        self.check(Verb::Read, Relation::Reader, path).await
    }

    /// Write access to `path`.
    pub async fn write(&self, path: &RepoPath) -> Result<()> {
        self.check(Verb::Write, Relation::Writer, path).await
    }

    /// Permission to vote and comment.
    pub async fn review(&self) -> Result<()> {
        self.check(Verb::Review, Relation::Reviewer, &RepoPathBuf::root())
            .await
    }

    /// Permission to land on trunk, revert and report verification.
    pub async fn land(&self) -> Result<()> {
        self.check(Verb::Land, Relation::Writer, &RepoPathBuf::root())
            .await
    }

    /// Repository administration: relations, policies, token issuance.
    pub async fn admin(&self) -> Result<()> {
        self.check(Verb::Admin, Relation::Owner, &RepoPathBuf::root())
            .await
    }

    /// Whether the caller may act on a record owned by `owner`: owners,
    /// repository owners and dev mode may; everyone else may not.
    pub async fn owns_or_admin(&self, owner: &rill_types::Principal) -> Result<()> {
        if owner.id == self.ctx.principal.id {
            return Ok(());
        }
        self.admin().await.map_err(|_| {
            Error::denied(format!(
                "owned by {owner}; only the owner or a repository owner may change it"
            ))
        })
    }

    /// Run both layers for `verb`/`relation` on `path`.
    pub async fn check(&self, verb: Verb, relation: Relation, path: &RepoPath) -> Result<()> {
        let owned = path.to_owned();
        if !self.ctx.capability_allows(self.repo.id(), &owned, verb) {
            return Err(Error::denied(format!(
                "token does not allow {verb} on {} in repository {}",
                describe(path),
                self.repo.config.name
            )));
        }
        if self.dev_mode || self.bootstrap().await? {
            return Ok(());
        }
        let allowed = self
            .repo
            .authorizer
            .check_path(&self.ctx.principal, &self.ctx.teams, relation, path)
            .await;
        if allowed {
            Ok(())
        } else {
            Err(Error::denied(format!(
                "{} is not {relation} of {} in repository {}",
                self.ctx.principal,
                describe(path),
                self.repo.config.name
            )))
        }
    }

    /// A filter over paths the caller may read, for listings and diffs
    /// that must not leak names of directories the caller cannot see.
    pub async fn read_filter(&self) -> Result<PathFilter> {
        if self.dev_mode || self.bootstrap().await? {
            return Ok(PathFilter::allow_all());
        }
        Ok(self
            .repo
            .authorizer
            .allowed_paths_for(&self.ctx.principal, &self.ctx.teams, Relation::Reader)
            .await)
    }

    /// The caller's kind as the `resolved_by` provenance value.
    pub fn resolved_by(&self) -> &'static str {
        match self.ctx.principal.kind {
            PrincipalKind::Human => "human",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Service => "service",
        }
    }

    async fn bootstrap(&self) -> Result<bool> {
        let snapshot = self.repo.authorizer.snapshot().await?;
        Ok(snapshot.tuples().is_empty())
    }
}

fn describe(path: &RepoPath) -> String {
    if path.is_root() {
        "the repository root".to_string()
    } else {
        format!("{path:?}")
    }
}
