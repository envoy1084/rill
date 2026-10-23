//! Org guardrails: `forbid` rules an org sets above every repository's
//! submit policy. Cedar lets a `forbid` win over any `permit`, so the
//! guardrails are appended to the repository's rules and no repository
//! rule can override them.

use cedar_policy::{Effect, PolicySet};

use super::policy::SubmitPolicy;
use crate::error::{Error, Result};

/// Every guardrail's `@id` starts with this, so a decision shows the org
/// stopped it and no repository rule can take the name.
pub const GUARDRAIL_PREFIX: &str = "org_";

/// Check guardrail text: only `forbid` rules, each named `org_…`, and
/// valid against the schema.
pub fn validate_guardrails(text: &str) -> Result<()> {
    let parsed: PolicySet = text
        .parse()
        .map_err(|e: cedar_policy::ParseErrors| Error::PolicyParse(e.to_string()))?;
    for policy in parsed.policies() {
        if policy.effect() != Effect::Forbid {
            return Err(Error::PolicyValidation(
                "org guardrails may only forbid".into(),
            ));
        }
        match policy.annotation("id") {
            Some(id) if id.starts_with(GUARDRAIL_PREFIX) => {}
            _ => {
                return Err(Error::PolicyValidation(format!(
                    "every guardrail needs an @id starting with {GUARDRAIL_PREFIX:?}"
                )))
            }
        }
    }
    SubmitPolicy::from_source(text).map(|_| ())
}

/// A repository's rules with the org's guardrails after them.
pub fn with_guardrails(repo: &str, guardrails: Option<&str>) -> String {
    match guardrails.filter(|g| !g.trim().is_empty()) {
        Some(g) => format!("{repo}\n// ---- org guardrails ----\n{g}\n"),
        None => repo.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use rill_types::{Principal, RepoPathBuf, RiskClass};

    use super::*;
    use crate::risk::{ChangeFacts, PathFacts};
    use crate::submit::{SubmitFacts, VoteSummary, DEFAULT_POLICY};

    #[test]
    fn a_guardrail_forbids_what_the_repository_permits() {
        let guard = r#"@id("org_no_weekend_agents")
forbid(principal, action, resource) when { resource.weekend && resource.author_kind == "agent" };"#;
        validate_guardrails(guard).unwrap();
        assert!(validate_guardrails("permit(principal, action, resource);").is_err());
        assert!(
            validate_guardrails("forbid(principal, action, resource);").is_err(),
            "unnamed"
        );
        let policy =
            SubmitPolicy::from_source(&with_guardrails(DEFAULT_POLICY, Some(guard))).unwrap();
        let mut change = ChangeFacts::new(
            Principal::agent("coder"),
            vec![PathFacts::modified(RepoPathBuf::new("a.rs").unwrap())],
        );
        let review = SubmitFacts::new(RiskClass::Normal).with_label(
            "Review",
            VoteSummary {
                max: 2,
                human_max: 2,
                human_votes: 1,
                human_approvals: 1,
                ..VoteSummary::default()
            },
        );
        let lander = Principal::human("ann");
        assert!(policy.evaluate(&lander, &change, &review).unwrap().allowed);
        change.at = rill_types::Timestamp::from_millis(2 * 86_400_000); // a Saturday
        let d = policy.evaluate(&lander, &change, &review).unwrap();
        assert!(!d.allowed);
        assert_eq!(d.matched_policies, ["org_no_weekend_agents"]);
    }
}
