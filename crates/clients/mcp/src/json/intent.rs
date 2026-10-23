//! Intents.

use rill_sdk::{IntentScopeExplanation, ScoredIntent};
use rill_types::{Intent, IntentScope};
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
        "scope": i.scope.as_ref().map(intent_scope),
        "revision": i.revision,
        "approvals": i.approvals.iter().map(|a| json!({
            "by": principal(&a.by),
            "at": timestamp(&a.at),
            "revision": a.revision,
            "owns": a.owns,
        })).collect::<Vec<_>>(),
    })
}

/// An intent's scope.
pub fn intent_scope(s: &IntentScope) -> Value {
    json!({
        "paths": s.paths,
        "forbid_paths": s.forbid_paths,
        "max_files_per_change": s.max_files_per_change,
        "max_changes": s.max_changes,
        "expires": s.expires.as_ref().map(timestamp),
        "agent_quorum": s.agent_quorum,
        "human_sample_permille": s.human_sample_permille,
        "changes_per_day": s.changes_per_day,
        "lands_per_day": s.lands_per_day,
    })
}

/// Whether a change inherits its intent's approval.
pub fn intent_scope_explanation(e: &IntentScopeExplanation) -> Value {
    json!({
        "intent": e.intent.map(|i| i.to_string()),
        "approved": e.approved,
        "in_scope": e.in_scope,
        "reasons": e.reasons,
        "missing_approvals": e.missing_approvals,
    })
}

/// A similarity hit.
pub fn scored_intent(s: &ScoredIntent) -> Value {
    json!({ "score": s.score, "intent": intent(&s.intent) })
}
