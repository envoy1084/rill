//! The submit policy a repository starts with.

/// Default submit rules in Cedar.
///
/// `forbid` always wins over `permit` in Cedar, so the blocking rules at the
/// top apply regardless of which permit matched. Every policy carries an
/// `@id` so [`crate::submit::SubmitDecision::matched_policies`] is readable.
pub const DEFAULT_POLICY: &str = r#"
// ---- blocking rules ----------------------------------------------------

@id("block_review_veto")
forbid(principal, action == Rill::Action::"land", resource)
when {
    resource.hasTag("Review") && resource.getTag("Review").min <= -2
};

@id("block_unresolved_bug_or_security")
forbid(principal, action == Rill::Action::"land", resource)
when { resource.unresolved_blocking_threads > 0 };

// Every owner set (OWNERS.toml) a change touches needs its approvals,
// unless the change is trivial or low risk.
@id("block_uncovered_owners")
forbid(principal, action == Rill::Action::"land", resource)
when {
    resource.owner_sets_missing > 0
    && resource.risk != "trivial" && resource.risk != "low"
};

// ---- permits by risk class ----------------------------------------------
//
// Agents review most changes: "independent" approvals are agent Review
// votes that are not the author's, not from its sponsor chain, from
// distinct model families or prompts, and not from demoted reviewers.
// A human's Review vote can stand in for them at every class.

// Trivial: a passing verification and one approval.
@id("land_trivial")
permit(principal, action == Rill::Action::"land", resource)
when {
    resource.risk == "trivial"
    && resource.hasTag("Verified") && resource.getTag("Verified").max >= 1
    && (resource.independent_agent_approvals >= 1
        || (resource.hasTag("Review") && resource.getTag("Review").human_max >= 1))
};

// Low: a passing verification and two independent agent approvals, or a
// human's.
@id("land_low")
permit(principal, action == Rill::Action::"land", resource)
when {
    resource.risk == "low"
    && resource.hasTag("Verified") && resource.getTag("Verified").max >= 1
    && (resource.independent_agent_approvals >= 2
        || (resource.hasTag("Review") && resource.getTag("Review").human_max >= 1))
};

// Normal: one human +2 on Review ...
@id("land_normal_human_review")
permit(principal, action == Rill::Action::"land", resource)
when {
    resource.risk == "normal"
    && resource.hasTag("Review") && resource.getTag("Review").human_max >= 2
};

// ... or a passing verification, two independent agent approvals (or the
// intent's quorum, if larger), and either an approved intent the change
// stays inside or a human owner's +1.
@id("land_normal_agent_review")
permit(principal, action == Rill::Action::"land", resource)
when {
    resource.risk == "normal"
    && resource.hasTag("Verified") && resource.getTag("Verified").max >= 1
    && resource.independent_agent_approvals >= 2
    && resource.independent_agent_approvals >= resource.intent_agent_quorum
    && (resource.intent_in_scope
        || (resource.hasTag("Owner") && resource.getTag("Owner").human_max >= 1))
};

// Sensitive: a human owner's +1, a passing verification and an independent
// agent review.
@id("land_sensitive_agent_review")
permit(principal, action == Rill::Action::"land", resource)
when {
    resource.risk == "sensitive"
    && resource.hasTag("Verified") && resource.getTag("Verified").max >= 1
    && resource.independent_agent_approvals >= 1
    && resource.hasTag("Owner") && resource.getTag("Owner").human_max >= 1
};

// Sensitive and critical: a human owner and a human +2.
@id("land_sensitive_or_critical")
permit(principal, action == Rill::Action::"land", resource)
when {
    (resource.risk == "sensitive" || resource.risk == "critical")
    && resource.hasTag("Owner") && resource.getTag("Owner").human_max >= 1
    && resource.hasTag("Review") && resource.getTag("Review").human_max >= 2
};
"#;

/// The default policy text.
pub fn default_policy() -> &'static str {
    DEFAULT_POLICY
}
