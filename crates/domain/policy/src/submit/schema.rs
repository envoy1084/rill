//! The Cedar schema that submit policies are validated against.

use std::sync::OnceLock;

use cedar_policy::Schema;

/// Namespace every entity and action lives in.
pub const NAMESPACE: &str = "Rill";

/// Cedar schema source for submit rules.
///
/// Labels are exposed as entity tags on `Rill::Change` so a policy can
/// address any label by name (`resource.getTag("Review")`) without the
/// schema having to enumerate the labels a repository defines. Every tag is
/// a `VoteSummary` aggregated over the votes on the current version.
pub const SCHEMA_SRC: &str = r#"
namespace Rill {
    // Aggregated votes on one label for the current version.
    type VoteSummary = {
        // Highest and lowest vote value across all voters; 0 when nobody voted.
        "max": Long,
        "min": Long,
        // Highest vote cast by a human / by an agent; 0 when none.
        "human_max": Long,
        "agent_max": Long,
        // Number of distinct humans / agents that voted at all.
        "human_votes": Long,
        "agent_votes": Long,
        // Number of distinct humans / agents that voted > 0.
        "human_approvals": Long,
        "agent_approvals": Long,
    };

    entity Principal {
        // "human", "agent" or "service".
        "kind": String,
        "name": String,
    };

    entity Change {
        // "trivial", "low", "normal", "sensitive" or "critical".
        "risk": String,
        "author_kind": String,
        // "feature", "bug", "refactor", "chore", "experiment", "system" or "none".
        "intent_kind": String,
        "unresolved_threads": Long,
        // Unresolved threads whose category is Bug or Security.
        "unresolved_blocking_threads": Long,
        "touches_sensitive": Bool,
        "has_agent_resolution": Bool,
        // Owner sets (from OWNERS.toml) the change touches, how many lack
        // their required approvals, and whether none does.
        "owner_sets": Long,
        "owner_sets_missing": Long,
        "owners_covered": Bool,
        // The linked intent is approved; the change is inside its approved
        // scope and budgets; the independent agent approvals it asks for
        // (0 when none or no intent).
        "intent_approved": Bool,
        "intent_in_scope": Bool,
        "intent_agent_quorum": Long,
        // Names of the labels present as tags, for `.contains` checks.
        "label_names": Set<String>,
    } tags VoteSummary;

    action land appliesTo {
        principal: [Principal],
        resource: [Change],
    };
}
"#;

/// The parsed schema, built once per process.
pub fn schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        // The source is a compile-time constant covered by tests, so a parse
        // failure here is a programming error, not a runtime condition.
        let (schema, _warnings) =
            Schema::from_cedarschema_str(SCHEMA_SRC).expect("built-in Cedar schema is valid");
        schema
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses() {
        let (_, warnings) = Schema::from_cedarschema_str(SCHEMA_SRC).unwrap();
        assert_eq!(warnings.count(), 0);
        let _ = schema();
    }
}
