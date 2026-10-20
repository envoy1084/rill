//! Intents.

use rill_sdk::ScoredIntent;
use rill_types::Intent;
use serde_json::{json, Value};

use super::primitives::{enum_name, principal, timestamp};

/// An intent record.
pub fn intent(i: &Intent) -> Value {
    json!({
        "id": i.id.to_string(),
        "title": i.title,
        "body": i.body,
        "kind": enum_name(&i.kind),
        "status": enum_name(&i.status),
        "owner": principal(&i.owner),
        "parent": i.parent.map(|p| p.to_string()),
        "risk_override": i.risk_override.map(|r| r.as_str()),
        "changes": i.changes.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        "created_at": timestamp(&i.created_at),
        "updated_at": timestamp(&i.updated_at),
    })
}

/// A similarity hit.
pub fn scored_intent(s: &ScoredIntent) -> Value {
    json!({ "score": s.score, "intent": intent(&s.intent) })
}
