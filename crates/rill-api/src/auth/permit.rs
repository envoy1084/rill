//! The permission check every handler runs before touching a repository.
//!
//! Every layer must agree:
//!
//! 1. the caller's capability token, if it has one, must allow the verb on
//!    the path (`AuthContext::capability_allows`); a token attenuates what
//!    the principal could otherwise do, so this holds even in dev mode;
//! 2. a registered agent is held to the repositories and paths its
//!    registration names;
//! 3. the ReBAC relations on the repository must grant the relation on
//!    the path's directory or an ancestor, or the caller administers the
//!    repository's organization (which makes it an owner of every
//!    repository there).
//!
//! The third layer is skipped in dev mode. Outside it a repository with no
//! tuples is closed to all but org administrators; `CreateRepo` makes its
//! creator the owner. Checks reflect at least the relation version the
//! caller says it has seen, so a grant or revoke is honoured by every
//! server from the next request of whoever made it.

use std::sync::Arc;

use rill_policy::rebac::Snapshot;
use rill_policy::{AuthContext, PathFilter, Relation, Verb};
use rill_types::{PrincipalKind, RepoPath, RepoPathBuf};
use tokio::sync::OnceCell;

use crate::error::{Error, Result};
use crate::state::RepoContext;

/// Permission checks for one caller on one repository.
pub struct Permit<'a> {
    ctx: &'a AuthContext,
    repo: &'a RepoContext,
    dev_mode: bool,
    org_admin: OnceCell<bool>,
}

impl<'a> Permit<'a> {
    /// Checks for `ctx` on `repo`.
    pub fn new(ctx: &'a AuthContext, repo: &'a RepoContext, dev_mode: bool) -> Self {
        Permit {
            ctx,
            repo,
            dev_mode,
            org_admin: OnceCell::new(),
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

    /// Run every layer for `verb`/`relation` on `path`.
    pub async fn check(&self, verb: Verb, relation: Relation, path: &RepoPath) -> Result<()> {
        let owned = path.to_owned();
        if !self.ctx.capability_allows(self.repo.id(), &owned, verb) {
            return Err(Error::denied(format!(
                "token does not allow {verb} on {} in repository {}",
                describe(path),
                self.repo.config.name
            )));
        }
        if let Some(scope) = &self.ctx.scope {
            let config = &self.repo.config;
            if !scope.covers_repo(&config.org, &config.name) || !scope.covers_path(path) {
                return Err(Error::denied(format!(
                    "agent {} is not registered for {} in repository {}",
                    self.ctx.principal,
                    describe(path),
                    config.name
                )));
            }
        }
        if self.dev_mode {
            return Ok(());
        }
        let object = rill_policy::rebac::dir_object(path);
        let snapshot = self.snapshot().await?;
        if snapshot.check(&self.ctx.principal, &self.ctx.teams, relation, &object)
            || self.org_admin().await?
        {
            return Ok(());
        }
        Err(Error::denied(format!(
            "{} is not {relation} of {} in repository {}",
            self.ctx.principal,
            describe(path),
            self.repo.config.name
        )))
    }

    /// A filter over paths the caller may read, for listings and diffs
    /// that must not leak names of directories the caller cannot see.
    pub async fn read_filter(&self) -> Result<PathFilter> {
        let relations = if self.dev_mode || self.org_admin().await? {
            PathFilter::allow_all()
        } else {
            let snapshot = self.snapshot().await?;
            PathFilter::under(snapshot.granted_dirs(
                &self.ctx.principal,
                &self.ctx.teams,
                Relation::Reader,
            ))
        };
        let relations = match self.ctx.capability.clone() {
            None => relations,
            Some(cap) => {
                if cap.repo != self.repo.id() || !cap.allows(Verb::Read) {
                    return Ok(PathFilter::deny_all());
                }
                relations.and(PathFilter::new(move |p| cap.covers_path(p)))
            }
        };
        Ok(match self.ctx.scope.clone() {
            None => relations,
            Some(scope) => {
                let config = &self.repo.config;
                if !scope.covers_repo(&config.org, &config.name) {
                    return Ok(PathFilter::deny_all());
                }
                relations.and(PathFilter::new(move |p| scope.covers_path(p)))
            }
        })
    }

    /// The caller's kind as the `resolved_by` provenance value.
    pub fn resolved_by(&self) -> &'static str {
        match self.ctx.principal.kind {
            PrincipalKind::Human => "human",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Service => "service",
        }
    }

    /// The tuples as of at least the version the caller has seen. A
    /// version the store has not reached is a forged or foreign token.
    async fn snapshot(&self) -> Result<Arc<Snapshot>> {
        let seen = self.ctx.relations_seen_in(self.repo.id());
        let snapshot = self.repo.authorizer.snapshot_at_least(seen).await?;
        if snapshot.version() < seen {
            return Err(Error::invalid(format!(
                "relation version {seen} is newer than repository {} has",
                self.repo.config.name
            )));
        }
        Ok(snapshot)
    }

    async fn org_admin(&self) -> Result<bool> {
        self.org_admin
            .get_or_try_init(|| {
                crate::auth::administers_org(
                    &self.repo.identity,
                    self.dev_mode,
                    self.ctx,
                    &self.repo.config.org,
                )
            })
            .await
            .copied()
    }
}

fn describe(path: &RepoPath) -> String {
    if path.is_root() {
        "the repository root".to_string()
    } else {
        format!("{path:?}")
    }
}
