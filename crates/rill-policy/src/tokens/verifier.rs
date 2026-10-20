//! Verifying a capability token against a concrete request.

use std::collections::HashMap;

use biscuit_auth::builder::{AuthorizerBuilder, Term};
use biscuit_auth::{Biscuit, PublicKey};
use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp, WorkspaceId};

use super::capability::{Capability, Verb};
use super::datalog::{effective_capability, principal_from_fact, PRINCIPAL_FACT};
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
    /// The token's effective capability.
    pub capability: Capability,
}

/// Checks tokens against the issuer's root public key.
#[derive(Debug, Clone)]
pub struct Verifier {
    root: PublicKey,
}

impl Verifier {
    /// Verifier for tokens signed by `root`.
    pub fn new(root: PublicKey) -> Self {
        Verifier { root }
    }

    /// Verifier from a 32-byte Ed25519 public key.
    pub fn from_public_key_bytes(bytes: &[u8]) -> Result<Self> {
        PublicKey::from_bytes(bytes, biscuit_auth::builder::Algorithm::Ed25519)
            .map(Self::new)
            .map_err(|e| Error::InvalidKey(e.to_string()))
    }

    /// The root public key.
    pub fn root(&self) -> &PublicKey {
        &self.root
    }

    /// Verify the signature chain and every block's checks against
    /// `request`, returning the token's principal.
    pub fn verify(&self, token: &str, request: &TokenRequest) -> Result<Principal> {
        self.verify_full(token, request).map(|v| v.principal)
    }

    /// Like [`Verifier::verify`] but also returns the effective capability.
    pub fn verify_full(&self, token: &str, request: &TokenRequest) -> Result<Verified> {
        let token =
            Biscuit::from_base64(token, self.root).map_err(|e| Error::Token(e.to_string()))?;
        let capability = effective_capability(&token)?;
        if capability.is_expired_at(request.now) {
            return Err(Error::TokenExpired {
                expires_at: capability.expires_at,
            });
        }

        let mut params: HashMap<String, Term> = HashMap::from([
            ("repo".to_string(), Term::Str(request.repo.to_string())),
            (
                "path".to_string(),
                Term::Str(request.path.as_str().to_string()),
            ),
            (
                "verb".to_string(),
                Term::Str(request.verb.as_str().to_string()),
            ),
            (
                "now".to_string(),
                Term::Date(request.now.millis().div_euclid(1000).max(0) as u64),
            ),
        ]);
        let mut code = String::from(
            "request_repo({repo});\nrequest_path({path});\nrequest_verb({verb});\ntime({now});\n",
        );
        if let Some(ws) = request.workspace {
            params.insert("ws".into(), Term::Str(ws.to_string()));
            code.push_str("request_workspace({ws});\n");
        }
        // All restrictions live in the token's checks; the policy only has
        // to accept once every check holds.
        code.push_str("allow if true;\n");

        let mut authorizer = AuthorizerBuilder::new()
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
        // attenuation block cannot inject a different principal.
        let facts: Vec<(String, String, String)> = authorizer
            .query(
                format!("data($id, $kind, $name) <- {PRINCIPAL_FACT}($id, $kind, $name)").as_str(),
            )
            .map_err(|e| Error::Token(e.to_string()))?;
        let (id, kind, name) = facts
            .first()
            .ok_or_else(|| Error::Token("token carries no principal".into()))?;
        Ok(Verified {
            principal: principal_from_fact(id, kind, name)?,
            capability,
        })
    }
}
