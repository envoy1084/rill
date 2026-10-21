//! Translation between capabilities and Biscuit Datalog.
//!
//! Every block of a token, the authority block and each attenuation, carries
//! the checks for its [`Capability`] and the capability's JSON in the block
//! context. Checks accumulate, so a child block can only ever remove
//! rights; the context lets a holder read back what a token grants without
//! re-deriving it from Datalog.

use std::collections::{BTreeSet, HashMap};

use biscuit_auth::builder::{BlockBuilder, Term};
use biscuit_auth::Biscuit;
use rill_types::{Principal, PrincipalKind, Timestamp};

use serde::{Deserialize, Serialize};

use super::capability::Capability;
use super::claims::TokenClaims;
use crate::error::{Error, Result};

/// Fact name carrying the token holder's identity.
pub const PRINCIPAL_FACT: &str = "principal";

fn token_err(e: impl std::fmt::Display) -> Error {
    Error::Token(e.to_string())
}

fn date(ts: Timestamp) -> Term {
    // Biscuit dates are whole seconds; round the expiry down so a token
    // never outlives its capability by a fraction of a second.
    Term::Date(ts.millis().div_euclid(1000).max(0) as u64)
}

/// Build the checks (and context) for one capability.
pub fn capability_block(cap: &Capability) -> Result<BlockBuilder> {
    cap.validate()?;
    let mut params: HashMap<String, Term> = HashMap::new();
    let mut code = String::new();

    params.insert("repo".into(), Term::Str(cap.repo.to_string()));
    code.push_str("check if request_repo({repo});\n");

    if let Some(ws) = cap.workspace {
        params.insert("ws".into(), Term::Str(ws.to_string()));
        code.push_str("check if request_workspace({ws});\n");
    }

    let verbs: BTreeSet<Term> = cap
        .verbs
        .iter()
        .map(|v| Term::Str(v.as_str().to_string()))
        .collect();
    params.insert("verbs".into(), Term::Set(verbs));
    code.push_str("check if request_verb($v), {verbs}.contains($v);\n");

    // A root prefix covers every path, so the check is omitted entirely.
    if !cap.paths.iter().any(|p| p.is_root()) {
        let mut clauses = Vec::with_capacity(cap.paths.len());
        for (i, path) in cap.paths.iter().enumerate() {
            params.insert(format!("p{i}"), Term::Str(path.as_str().to_string()));
            params.insert(format!("d{i}"), Term::Str(format!("{}/", path.as_str())));
            clauses.push(format!("$p == {{p{i}}} || $p.starts_with({{d{i}}})"));
        }
        code.push_str(&format!(
            "check if request_path($p), {};\n",
            clauses.join(" || ")
        ));
    }

    params.insert("exp".into(), date(cap.expires_at));
    code.push_str("check if time($t), $t < {exp};\n");

    let context = serde_json::to_string(cap).map_err(token_err)?;
    BlockBuilder::new()
        .code_with_params(code, params, HashMap::new())
        .map_err(token_err)
        .map(|b| b.context(context))
}

/// Fact name carrying the token's revocation id.
pub const TOKEN_FACT: &str = "token";

/// The authority block's context: the claims and the capability, if the
/// token is narrowed to one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityContext {
    pub claims: TokenClaims,
    pub capability: Option<Capability>,
}

/// The authority block: the holder's identity, the token id, the expiry
/// check and, when the token is narrowed, the capability checks.
pub fn authority_block(
    principal: &Principal,
    claims: &TokenClaims,
    capability: Option<&Capability>,
) -> Result<BlockBuilder> {
    if let Some(cap) = capability {
        if cap.expires_at > claims.expires_at {
            return Err(Error::Widening("the capability outlives the token".into()));
        }
    }
    let params = HashMap::from([
        ("id".to_string(), Term::Str(principal.id.to_string())),
        (
            "kind".to_string(),
            Term::Str(principal.kind.as_str().to_string()),
        ),
        ("name".to_string(), Term::Str(principal.name.clone())),
        ("tid".to_string(), Term::Str(claims.token_id.to_string())),
        ("exp".to_string(), date(claims.expires_at)),
    ]);
    let identity = BlockBuilder::new()
        .code_with_params(
            format!(
                "{PRINCIPAL_FACT}({{id}}, {{kind}}, {{name}});\n\
                 {TOKEN_FACT}({{tid}});\n\
                 check if time($t), $t < {{exp}};"
            ),
            params,
            HashMap::new(),
        )
        .map_err(token_err)?;
    let block = match capability {
        Some(cap) => identity.merge(capability_block(cap)?),
        None => identity,
    };
    let context = serde_json::to_string(&AuthorityContext {
        claims: claims.clone(),
        capability: capability.cloned(),
    })
    .map_err(token_err)?;
    Ok(block.context(context))
}

/// What a verified token says: its claims and its effective capability,
/// the one of its last block after checking that every block narrows the
/// one before it. `None` means the token is not narrowed.
pub fn effective_scope(token: &Biscuit) -> Result<(TokenClaims, Option<Capability>)> {
    let mut contexts = token.context().into_iter();
    let authority = contexts
        .next()
        .flatten()
        .ok_or_else(|| Error::Token("token has no authority context".into()))?;
    let AuthorityContext {
        claims,
        capability: mut effective,
    } = serde_json::from_str(&authority)
        .map_err(|e| Error::Token(format!("authority context: {e}")))?;
    for (i, ctx) in contexts.enumerate() {
        let ctx = ctx.ok_or_else(|| Error::Token(format!("block {} has no capability", i + 1)))?;
        let cap: Capability = serde_json::from_str(&ctx)
            .map_err(|e| Error::Token(format!("block {} capability: {e}", i + 1)))?;
        match &effective {
            Some(parent) => cap.check_narrower_than(parent)?,
            None if cap.expires_at > claims.expires_at => {
                return Err(Error::Widening("the capability outlives the token".into()))
            }
            None => {}
        }
        effective = Some(cap);
    }
    Ok((claims, effective))
}

/// Rebuild a principal from the identity fact's terms.
pub fn principal_from_fact(id: &str, kind: &str, name: &str) -> Result<Principal> {
    let kind = match kind {
        "human" => PrincipalKind::Human,
        "agent" => PrincipalKind::Agent,
        "service" => PrincipalKind::Service,
        other => return Err(Error::Token(format!("unknown principal kind {other:?}"))),
    };
    Ok(Principal {
        id: id.parse().map_err(token_err)?,
        kind,
        name: name.to_string(),
        identity: None,
    })
}
