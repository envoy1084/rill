//! Verifying a capability token against a concrete request.

use std::collections::HashMap;

use biscuit_auth::builder::{AuthorizerBuilder, Term};
use biscuit_auth::Biscuit;
use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp, WorkspaceId};

use super::capability::{Capability, Verb};
use super::claims::TokenClaims;
use super::datalog::{effective_scope, principal_from_fact, PRINCIPAL_FACT, TOKEN_FACT};
use super::keyring::KeyRing;
use crate::error::{Error, Result};

/// The request a token is checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRequest {
    /// Repository being accessed.
    pub repo: RepoId,
    /// Workspace being accessed, if the operation is workspace-scoped.
    pub workspace: Option<WorkspaceId>,
    /// Path being accessed; the root for repository-wide operations.
    pub path: RepoPathBuf,
    /// Operation class.
    pub verb: Verb,
    /// The current time, supplied by the caller for determinism.
    pub now: Timestamp,
}

/// What a successful verification establishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The principal the token was issued to.
    pub principal: Principal,
    /// The signed claims of the authority block.
    pub claims: TokenClaims,
    /// The token's effective capability; `None` when it is not narrowed.
    pub capability: Option<Capability>,
}

/// Biscuit's default allows 1 ms of wall-clock time per authorization,
/// which a preempted thread on a busy server exceeds, refusing valid
/// tokens. Facts and iterations stay bounded, which is what stops a
/// degenerate token; time only has to catch a runaway.
const AUTHORIZER_LIMITS: biscuit_auth::AuthorizerLimits = biscuit_auth::AuthorizerLimits {
    max_facts: 1000,
    max_iterations: 100,
    max_time: std::time::Duration::from_millis(250),
};

/// Checks tokens against the trusted signing keys.
#[derive(Debug, Clone)]
pub struct Verifier {
    keys: KeyRing,
}

impl Verifier {
    /// Verifier trusting `keys`.
    pub fn new(keys: KeyRing) -> Self {
        Verifier { keys }
    }

    pub fn keys(&self) -> &KeyRing {
        &self.keys
    }

    /// Verify the signature chain and every block's checks against
    /// `request`, returning the token's principal.
    pub fn verify(&self, token: &str, request: &TokenRequest) -> Result<Principal> {
        self.verify_full(token, request).map(|v| v.principal)
    }

    /// Verify a token without a concrete request: the signature chain, the
    /// expiry and a consistent block chain, probing the capability with
    /// something it claims to allow. Per-request checks happen when the
    /// request runs, against the returned capability.
    pub fn authenticate(&self, token: &str, now: Timestamp) -> Result<Verified> {
        let (_, capability) = super::issuer::inspect_at(token, &self.keys, now)?;
        let probe = capability.as_ref().map(|cap| TokenRequest {
            repo: cap.repo,
            workspace: cap.workspace,
            path: cap.paths.first().cloned().unwrap_or_else(RepoPathBuf::root),
            verb: cap.verbs.first().copied().unwrap_or(Verb::Read),
            now,
        });
        self.check(token, probe.as_ref(), now)
    }

    /// Like [`Verifier::verify`] but also returns the claims and capability.
    pub fn verify_full(&self, token: &str, request: &TokenRequest) -> Result<Verified> {
        self.check(token, Some(request), request.now)
    }

    fn check(
        &self,
        token: &str,
        request: Option<&TokenRequest>,
        now: Timestamp,
    ) -> Result<Verified> {
        let token = Biscuit::from_base64(token, |id| {
            self.keys.key_at(id, now).map_err(super::issuer::key_err)
        })
        .map_err(|e| Error::Token(e.to_string()))?;
        let (claims, capability) = effective_scope(&token)?;
        let expires_at = capability.as_ref().map_or(claims.expires_at, |cap| {
            cap.expires_at.min(claims.expires_at)
        });
        if now >= expires_at {
            return Err(Error::TokenExpired { expires_at });
        }

        let mut params: HashMap<String, Term> = HashMap::from([(
            "now".to_string(),
            Term::Date(now.millis().div_euclid(1000).max(0) as u64),
        )]);
        let mut code = String::from("time({now});\n");
        if let Some(request) = request {
            params.insert("repo".into(), Term::Str(request.repo.to_string()));
            params.insert("path".into(), Term::Str(request.path.as_str().to_string()));
            params.insert("verb".into(), Term::Str(request.verb.as_str().to_string()));
            code.push_str("request_repo({repo});\nrequest_path({path});\nrequest_verb({verb});\n");
            if let Some(ws) = request.workspace {
                params.insert("ws".into(), Term::Str(ws.to_string()));
                code.push_str("request_workspace({ws});\n");
            }
        }
        // All restrictions live in the token's checks; the policy only has
        // to accept once every check holds.
        code.push_str("allow if true;\n");

        let mut authorizer = AuthorizerBuilder::new()
            .set_limits(AUTHORIZER_LIMITS)
            .code_with_params(code, params, HashMap::new())
            .map_err(|e| Error::Token(e.to_string()))?
            .build(&token)
            .map_err(|e| Error::Token(e.to_string()))?;
        authorizer.authorize().map_err(|e| match e {
            biscuit_auth::error::Token::FailedLogic(logic) => {
                Error::TokenRejected(logic.to_string())
            }
            other => Error::Token(other.to_string()),
        })?;

        // `query` (not `query_all`) sees only the authority block, so an
        // attenuation block cannot inject a different principal or token id.
        let facts: Vec<(String, String, String)> = authorizer
            .query(
                format!("data($id, $kind, $name) <- {PRINCIPAL_FACT}($id, $kind, $name)").as_str(),
            )
            .map_err(|e| Error::Token(e.to_string()))?;
        let (id, kind, name) = facts
            .first()
            .ok_or_else(|| Error::Token("token carries no principal".into()))?;
        let ids: Vec<(String,)> = authorizer
            .query(format!("data($id) <- {TOKEN_FACT}($id)").as_str())
            .map_err(|e| Error::Token(e.to_string()))?;
        if ids.first().map(|(id,)| id.as_str()) != Some(claims.token_id.to_string().as_str()) {
            return Err(Error::Token("token id does not match its claims".into()));
        }
        Ok(Verified {
            principal: principal_from_fact(id, kind, name)?,
            claims,
            capability,
        })
    }
}
