//! Principal resolution: turning request credentials into an [`AuthContext`].
//!
//! The server adapts its transport metadata into [`AuthHeaders`] and hands
//! them to an [`Authenticator`]; this crate never sees tonic or HTTP types.

use std::collections::HashMap;

use async_trait::async_trait;
use rill_types::{Principal, RepoId, RepoPath, RepoPathBuf};

use crate::error::{Error, Result};
use crate::tokens::{Capability, TokenClaims, Verb};

/// Credentials extracted from a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthHeaders {
    /// The bearer token, with the `Bearer ` prefix already stripped.
    pub bearer: Option<String>,
    /// A principal name the client claims; only honored by authenticators
    /// that trust the transport (none of the built-in ones do).
    pub principal_hint: Option<String>,
}

impl AuthHeaders {
    /// Headers carrying only a bearer token.
    pub fn bearer(token: impl Into<String>) -> Self {
        AuthHeaders {
            bearer: Some(token.into()),
            principal_hint: None,
        }
    }
}

/// What a request runs as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    /// The authenticated principal.
    pub principal: Principal,
    /// Teams the principal belongs to, for ReBAC team subjects.
    pub teams: Vec<String>,
    /// The capability the request's token carries, if it carried one.
    /// `None` means the principal is bound only by relationship checks.
    pub capability: Option<Capability>,
    /// The signed claims of the request's token; `None` for static tokens
    /// and dev mode.
    pub claims: Option<TokenClaims>,
    /// What a registered agent may touch; `None` for everyone else.
    pub scope: Option<AgentScope>,
    /// Relation versions the caller has observed, by repository: checks
    /// on those repositories must reflect at least that version.
    pub relations_seen: Vec<(RepoId, i64)>,
}

/// The repositories and paths a registered agent may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScope {
    pub org: String,
    /// Repository names in `org`; `*` for all.
    pub repos: Vec<String>,
    /// Path prefixes; empty for every path.
    pub paths: Vec<RepoPathBuf>,
    /// What it may spend.
    pub budgets: rill_types::identity::AgentBudgets,
}

impl AgentScope {
    /// Whether the scope covers `repo` of `org`.
    pub fn covers_repo(&self, org: &str, repo: &str) -> bool {
        org == self.org && self.repos.iter().any(|r| r == "*" || r == repo)
    }

    /// Whether the scope covers `path`.
    pub fn covers_path(&self, path: &RepoPath) -> bool {
        self.paths.is_empty() || self.paths.iter().any(|p| path.starts_with(p))
    }
}

impl AuthContext {
    /// The relation version checks on `repo` must reflect; 0 if none.
    pub fn relations_seen_in(&self, repo: RepoId) -> i64 {
        self.relations_seen
            .iter()
            .filter(|(r, _)| *r == repo)
            .map(|(_, v)| *v)
            .max()
            .unwrap_or(0)
    }

    /// Whether the context's capability (if any) permits `verb` on `path`
    /// in `repo`. Without a capability this is `true`; callers must still
    /// run relationship checks.
    pub fn capability_allows(&self, repo: RepoId, path: &RepoPathBuf, verb: Verb) -> bool {
        match &self.capability {
            None => true,
            Some(cap) => cap.repo == repo && cap.allows(verb) && cap.covers_path(path),
        }
    }
}

/// Resolves credentials to a principal.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Authenticate a request. Returns [`Error::Unauthenticated`] when no
    /// acceptable credential is present.
    async fn authenticate(&self, headers: &AuthHeaders) -> Result<AuthContext>;
}

/// A fixed map of bearer tokens to principals, for single-node and
/// development deployments.
#[derive(Debug, Default)]
pub struct StaticTokenAuthenticator {
    tokens: HashMap<String, (Principal, Vec<String>)>,
}

impl StaticTokenAuthenticator {
    /// An empty map; every request is rejected until tokens are added.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a token for `principal` with the given teams.
    pub fn insert(
        &mut self,
        token: impl Into<String>,
        principal: Principal,
        teams: Vec<String>,
    ) -> &mut Self {
        self.tokens.insert(token.into(), (principal, teams));
        self
    }

    /// Builder-style [`StaticTokenAuthenticator::insert`].
    pub fn with(
        mut self,
        token: impl Into<String>,
        principal: Principal,
        teams: Vec<String>,
    ) -> Self {
        self.insert(token, principal, teams);
        self
    }

    /// Number of registered tokens.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Whether no tokens are registered.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

#[async_trait]
impl Authenticator for StaticTokenAuthenticator {
    async fn authenticate(&self, headers: &AuthHeaders) -> Result<AuthContext> {
        let token = headers
            .bearer
            .as_deref()
            .ok_or_else(|| Error::Unauthenticated("missing bearer token".into()))?;
        let (principal, teams) = self
            .tokens
            .get(token)
            .ok_or_else(|| Error::Unauthenticated("unknown bearer token".into()))?;
        Ok(AuthContext {
            principal: principal.clone(),
            teams: teams.clone(),
            capability: None,
            claims: None,
            scope: None,
            relations_seen: Vec::new(),
        })
    }
}

/// Looks up the teams a principal belongs to.
#[async_trait]
pub trait TeamResolver: Send + Sync {
    /// Teams of `principal`, possibly empty.
    async fn teams_of(&self, principal: &Principal) -> Result<Vec<String>>;
}

/// A resolver that returns no teams.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoTeams;

#[async_trait]
impl TeamResolver for NoTeams {
    async fn teams_of(&self, _principal: &Principal) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::Timestamp;

    #[tokio::test]
    async fn static_tokens() {
        let auth = StaticTokenAuthenticator::new().with(
            "secret",
            Principal::human("alice"),
            vec!["core".into()],
        );
        let ctx = auth
            .authenticate(&AuthHeaders::bearer("secret"))
            .await
            .unwrap();
        assert_eq!(ctx.principal, Principal::human("alice"));
        assert_eq!(ctx.teams, vec!["core"]);
        assert!(ctx.capability.is_none());
        assert!(ctx.capability_allows(RepoId::new(), &RepoPathBuf::root(), Verb::Admin));

        assert!(matches!(
            auth.authenticate(&AuthHeaders::bearer("nope")).await,
            Err(Error::Unauthenticated(_))
        ));
        assert!(matches!(
            auth.authenticate(&AuthHeaders::default()).await,
            Err(Error::Unauthenticated(_))
        ));
    }

    #[test]
    fn a_capability_narrows_what_a_context_allows() {
        let repo = RepoId::derived("t", "r");
        let cap = Capability::repo_wide(repo, [Verb::Read], Timestamp::from_millis(i64::MAX))
            .with_paths([RepoPathBuf::new("src").unwrap()]);
        let ctx = AuthContext {
            principal: Principal::agent("bot"),
            teams: Vec::new(),
            capability: Some(cap),
            claims: None,
            scope: None,
            relations_seen: Vec::new(),
        };
        assert!(ctx.capability_allows(repo, &RepoPathBuf::new("src/x").unwrap(), Verb::Read));
        assert!(!ctx.capability_allows(repo, &RepoPathBuf::new("src/x").unwrap(), Verb::Write));
        assert!(!ctx.capability_allows(repo, &RepoPathBuf::new("docs").unwrap(), Verb::Read));
        assert!(!ctx.capability_allows(
            RepoId::new(),
            &RepoPathBuf::new("src/x").unwrap(),
            Verb::Read
        ));
    }
}
