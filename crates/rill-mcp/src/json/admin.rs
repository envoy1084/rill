//! Identity, tokens, relations, policies and health.

use rill_sdk::{Capability, Health, IssuedToken, Policy, Relation, WhoAmI};
use serde_json::{json, Value};

use super::primitives::{opt, principal, timestamp};

/// A capability.
pub fn capability(c: &Capability) -> Value {
    json!({
        "action": c.action,
        "scope": c.scope,
        "expires_at": opt(c.expires_at.as_ref(), timestamp),
    })
}

/// The caller's identity.
pub fn whoami(w: &WhoAmI) -> Value {
    json!({
        "principal": principal(&w.principal),
        "teams": w.teams,
        "capabilities": w.capabilities.iter().map(capability).collect::<Vec<_>>(),
        "token_expires_at": opt(w.token_expires_at.as_ref(), timestamp),
    })
}

/// A minted token.
pub fn issued_token(t: &IssuedToken) -> Value {
    json!({ "token": t.token, "expires_at": timestamp(&t.expires_at) })
}

/// A ReBAC tuple.
pub fn relation(r: &Relation) -> Value {
    json!({ "object": r.object, "relation": r.relation, "subject": r.subject })
}

/// A named policy.
pub fn policy(p: &Policy) -> Value {
    json!({
        "name": p.name,
        "text": p.text,
        "updated_at": timestamp(&p.updated_at),
        "updated_by": principal(&p.updated_by),
    })
}

/// Server health.
pub fn health(h: &Health) -> Value {
    json!({
        "ok": h.ok,
        "version": h.version,
        "components": h.components.iter().map(|c| json!({
            "name": c.name,
            "ok": c.ok,
            "detail": c.detail,
        })).collect::<Vec<_>>(),
    })
}
