//! Principal resolution: turning request credentials into an [`AuthContext`].
//!
//! The server adapts its transport metadata into [`AuthHeaders`] and hands
//! them to an [`Authenticator`]; this crate never sees tonic or HTTP types.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp};

use crate::error::{Error, Result};
use crate::tokens::{Capability, TokenRequest, Verb, Verifier};

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
}

impl AuthContext {
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

/// Authenticates Biscuit capability tokens issued by this instance.
///
/// Authentication establishes identity and the token's capability; whether
/// the capability covers a particular operation is checked later by the
/// server per request via [`Verifier::verify`] or
/// [`AuthContext::capability_allows`]. The signature chain and expiry are
/// checked here using a probe request against the capability's own scope.
pub struct BiscuitAuthenticator {
    verifier: Verifier,
    teams: Arc<dyn TeamResolver>,
}

impl std::fmt::Debug for BiscuitAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiscuitAuthenticator")
            .finish_non_exhaustive()
    }
}

impl BiscuitAuthenticator {
    /// Authenticator with no team membership.
    pub fn new(verifier: Verifier) -> Self {
        Self::with_teams(verifier, Arc::new(NoTeams))
    }

    /// Authenticator that fills in teams from `teams`.
    pub fn with_teams(verifier: Verifier, teams: Arc<dyn TeamResolver>) -> Self {
        BiscuitAuthenticator { verifier, teams }
    }

    /// The underlying verifier, for per-request capability checks.
    pub fn verifier(&self) -> &Verifier {
        &self.verifier
    }
}

#[async_trait]
impl Authenticator for BiscuitAuthenticator {
    async fn authenticate(&self, headers: &AuthHeaders) -> Result<AuthContext> {
        let token = headers
            .bearer
            .as_deref()
            .ok_or_else(|| Error::Unauthenticated("missing bearer token".into()))?;
        let capability = crate::tokens::inspect(token, self.verifier.root())
            .map_err(|e| Error::Unauthenticated(e.to_string()))?;
        // Probe with something the capability itself claims to allow, so
        // that only the signature, expiry and a consistent block chain are
        // being tested here.
        let probe = TokenRequest {
            repo: capability.repo,
            workspace: capability.workspace,
            path: capability
                .paths
                .first()
                .cloned()
                .unwrap_or_else(RepoPathBuf::root),
            verb: capability.verbs.first().copied().unwrap_or(Verb::Read),
            now: Timestamp::now(),
        };
        let verified = self
            .verifier
            .verify_full(token, &probe)
            .map_err(|e| match e {
                Error::TokenExpired { .. } => e,
                other => Error::Unauthenticated(other.to_string()),
            })?;
        let teams = self.teams.teams_of(&verified.principal).await?;
        Ok(AuthContext {
            principal: verified.principal,
            teams,
            capability: Some(verified.capability),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::Issuer;

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

    struct FixedTeams;

    #[async_trait]
    impl TeamResolver for FixedTeams {
        async fn teams_of(&self, _p: &Principal) -> Result<Vec<String>> {
            Ok(vec!["agents".into()])
        }
    }

    #[tokio::test]
    async fn biscuit_tokens() {
        let issuer = Issuer::generate();
        let auth = BiscuitAuthenticator::with_teams(
            Verifier::new(issuer.public_key()),
            Arc::new(FixedTeams),
        );
        let repo = RepoId::derived("t", "r");
        let bot = Principal::agent("bot");
        let cap = Capability::repo_wide(
            repo,
            [Verb::Read],
            Timestamp::from_millis(Timestamp::now().millis() + 60_000),
        )
        .with_paths([RepoPathBuf::new("src").unwrap()]);
        let token = issuer.issue(&bot, &cap).unwrap();

        let ctx = auth
            .authenticate(&AuthHeaders::bearer(&token))
            .await
            .unwrap();
        assert_eq!(ctx.principal, bot);
        assert_eq!(ctx.teams, vec!["agents"]);
        assert_eq!(ctx.capability.as_ref(), Some(&cap));
        assert!(ctx.capability_allows(repo, &RepoPathBuf::new("src/x").unwrap(), Verb::Read));
        assert!(!ctx.capability_allows(repo, &RepoPathBuf::new("src/x").unwrap(), Verb::Write));
        assert!(!ctx.capability_allows(repo, &RepoPathBuf::new("docs").unwrap(), Verb::Read));

        let expired = issuer
            .issue(
                &bot,
                &Capability::repo_wide(repo, [Verb::Read], Timestamp::from_millis(1)),
            )
            .unwrap();
        assert!(matches!(
            auth.authenticate(&AuthHeaders::bearer(expired)).await,
            Err(Error::TokenExpired { .. })
        ));
        assert!(matches!(
            auth.authenticate(&AuthHeaders::bearer("garbage")).await,
            Err(Error::Unauthenticated(_))
        ));
    }
}
