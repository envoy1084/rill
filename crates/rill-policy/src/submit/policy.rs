//! Loading, validating and evaluating a Cedar submit policy.

use cedar_policy::{Authorizer, Decision, Effect, PolicyId, PolicySet, ValidationMode};
use rill_types::Principal;
use serde::{Deserialize, Serialize};

use super::default_policy::DEFAULT_POLICY;
use super::entities::build_request;
use super::facts::SubmitFacts;
use super::schema::schema;
use crate::error::{Error, Result};
use crate::risk::ChangeFacts;

/// The outcome of evaluating submit rules for one change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitDecision {
    /// Whether the change may land.
    pub allowed: bool,
    /// Ids of the policies that determined the decision: the forbids that
    /// fired when denied, otherwise the permits that fired.
    pub matched_policies: Vec<String>,
    /// Human-readable explanation, one line per matched policy, plus any
    /// evaluation errors Cedar reported.
    pub reasons: Vec<String>,
}

/// A validated set of Cedar submit rules.
#[derive(Debug, Clone)]
pub struct SubmitPolicy {
    policies: PolicySet,
    digest: [u8; 32],
}

impl SubmitPolicy {
    /// Parse Cedar policy text and validate it against the submit schema in
    /// strict mode. Policies that reference unknown attributes or compare
    /// mismatched types are rejected here rather than failing at land time.
    pub fn from_source(source: &str) -> Result<Self> {
        let parsed: PolicySet = source
            .parse()
            .map_err(|e: cedar_policy::ParseErrors| Error::PolicyParse(e.to_string()))?;
        let policies = apply_id_annotations(parsed)?;
        let validator = cedar_policy::Validator::new(schema().clone());
        let result = validator.validate(&policies, ValidationMode::Strict);
        if !result.validation_passed() {
            let errors: Vec<String> = result.validation_errors().map(|e| e.to_string()).collect();
            return Err(Error::PolicyValidation(errors.join("; ")));
        }
        Ok(SubmitPolicy {
            policies,
            digest: *blake3::hash(source.as_bytes()).as_bytes(),
        })
    }

    /// The built-in default rules.
    pub fn default_rules() -> Self {
        // The default text is covered by tests; failing to parse it would be
        // a bug in this crate.
        Self::from_source(DEFAULT_POLICY).expect("default submit policy is valid")
    }

    /// BLAKE3 of the policy text: which rules a decision came from.
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Number of policies in the set.
    pub fn len(&self) -> usize {
        self.policies.policies().count()
    }

    /// Whether the set has no policies (which denies everything).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Evaluate whether `principal` may land a change with these facts.
    /// Synchronous and allocation-light; safe to call on every vote.
    pub fn evaluate(
        &self,
        principal: &Principal,
        change: &ChangeFacts,
        review: &SubmitFacts,
    ) -> Result<SubmitDecision> {
        let (request, entities) = build_request(principal, change, review)?;
        let response = Authorizer::new().is_authorized(&request, &self.policies, &entities);
        let allowed = response.decision() == Decision::Allow;

        let mut matched: Vec<(String, Option<String>)> = response
            .diagnostics()
            .reason()
            .map(|id| (id.to_string(), self.describe(id)))
            .collect();
        matched.sort();

        let mut reasons: Vec<String> = matched
            .iter()
            .map(|(id, desc)| match desc {
                Some(d) => format!("{id}: {d}"),
                None => id.clone(),
            })
            .collect();
        if matched.is_empty() {
            reasons.push("no permit rule matched".into());
        }
        for err in response.diagnostics().errors() {
            tracing::warn!(error = %err, "submit policy evaluation error");
            reasons.push(format!("evaluation error: {err}"));
        }

        Ok(SubmitDecision {
            allowed,
            matched_policies: matched.into_iter().map(|(id, _)| id).collect(),
            reasons,
        })
    }

    fn describe(&self, id: &PolicyId) -> Option<String> {
        let policy = self.policies.policy(id)?;
        let effect = match policy.effect() {
            Effect::Permit => "permit",
            Effect::Forbid => "forbid",
        };
        Some(match policy.annotation("description") {
            Some(d) => format!("{effect} ({d})"),
            None => effect.to_string(),
        })
    }
}

/// Cedar assigns `policy0..N` ids when parsing text; an `@id("...")`
/// annotation renames the policy so decisions can cite it by name.
fn apply_id_annotations(parsed: PolicySet) -> Result<PolicySet> {
    let mut out = PolicySet::new();
    for policy in parsed.policies() {
        let renamed = match policy.annotation("id") {
            Some(id) => policy.new_id(PolicyId::new(id)),
            None => policy.clone(),
        };
        out.add(renamed)
            .map_err(|e| Error::PolicyParse(format!("duplicate policy id: {e}")))?;
    }
    Ok(out)
}

impl Default for SubmitPolicy {
    fn default() -> Self {
        Self::default_rules()
    }
}
