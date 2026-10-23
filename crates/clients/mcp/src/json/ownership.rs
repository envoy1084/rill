//! Code owners.

use rill_sdk::{OwnerSet, Ownership};
use serde_json::{json, Value};

/// An owner set.
pub fn owner_set(s: &OwnerSet) -> Value {
    json!({
        "id": s.id,
        "dir": s.dir,
        "owners": s.owners,
        "required": s.required,
        "sensitive": s.sensitive,
        "sla_ms": s.sla.map(|d| d.as_millis() as u64),
        "escalate_to": s.escalate_to,
    })
}

/// A change's standing with each owner set it touches.
pub fn ownership(o: &Ownership) -> Value {
    json!({
        "covered": o.covered,
        "sets": o.sets.iter().map(|s| json!({
            "set": owner_set(&s.set),
            "approvals": s.approvals,
            "covered": s.covered,
            "escalated_to": s.escalated_to,
            "escalations": s.escalations,
        })).collect::<Vec<_>>(),
    })
}
