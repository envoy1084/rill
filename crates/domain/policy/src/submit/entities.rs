//! Translation of Rill facts into Cedar entities and requests.

use std::collections::HashMap;

use cedar_policy::{Context, Entities, Entity, EntityId, EntityUid, Request, RestrictedExpression};
use rill_types::{IntentKind, Principal};

use super::facts::{SubmitFacts, VoteSummary};
use super::schema::{schema, NAMESPACE};
use crate::error::{Error, Result};
use crate::risk::ChangeFacts;

fn uid(ty: &str, id: &str) -> Result<EntityUid> {
    let type_name = format!("{NAMESPACE}::{ty}")
        .parse()
        .map_err(|e| Error::PolicyEvaluation(format!("entity type {ty}: {e}")))?;
    Ok(EntityUid::from_type_name_and_id(
        type_name,
        EntityId::new(id),
    ))
}

/// Uid of the `land` action.
pub fn land_action() -> Result<EntityUid> {
    uid("Action", "land")
}

fn string(s: &str) -> RestrictedExpression {
    RestrictedExpression::new_string(s.to_string())
}

fn long(n: i64) -> RestrictedExpression {
    RestrictedExpression::new_long(n)
}

fn vote_summary(s: &VoteSummary) -> Result<RestrictedExpression> {
    RestrictedExpression::new_record([
        ("max".to_string(), long(s.max.into())),
        ("min".to_string(), long(s.min.into())),
        ("human_max".to_string(), long(s.human_max.into())),
        ("agent_max".to_string(), long(s.agent_max.into())),
        ("human_votes".to_string(), long(s.human_votes.into())),
        ("agent_votes".to_string(), long(s.agent_votes.into())),
        (
            "human_approvals".to_string(),
            long(s.human_approvals.into()),
        ),
        (
            "agent_approvals".to_string(),
            long(s.agent_approvals.into()),
        ),
    ])
    .map_err(|e| Error::PolicyEvaluation(e.to_string()))
}

fn intent_kind_str(kind: Option<IntentKind>) -> &'static str {
    match kind {
        Some(IntentKind::Feature) => "feature",
        Some(IntentKind::Bug) => "bug",
        Some(IntentKind::Refactor) => "refactor",
        Some(IntentKind::Chore) => "chore",
        Some(IntentKind::Experiment) => "experiment",
        Some(IntentKind::System) => "system",
        None => "none",
    }
}

/// The principal entity for the actor requesting the land.
pub fn principal_entity(principal: &Principal) -> Result<(EntityUid, Entity)> {
    let id = uid("Principal", &principal.id.to_string())?;
    let attrs = HashMap::from([
        ("kind".to_string(), string(principal.kind.as_str())),
        ("name".to_string(), string(&principal.name)),
    ]);
    let entity = Entity::new(id.clone(), attrs, Default::default())
        .map_err(|e| Error::PolicyEvaluation(e.to_string()))?;
    Ok((id, entity))
}

/// The change entity, with one tag per label.
pub fn change_entity(change: &ChangeFacts, review: &SubmitFacts) -> Result<(EntityUid, Entity)> {
    let id = uid("Change", "current")?;
    let label_names = RestrictedExpression::new_set(review.label_names().into_iter().map(string));
    let attrs = [
        ("risk", string(review.risk.as_str())),
        ("author_kind", string(change.author.kind.as_str())),
        ("intent_kind", string(intent_kind_str(change.intent_kind))),
        ("unresolved_threads", long(review.unresolved_threads.into())),
        (
            "unresolved_blocking_threads",
            long(review.unresolved_blocking_threads.into()),
        ),
        (
            "touches_sensitive",
            RestrictedExpression::new_bool(review.touches_sensitive),
        ),
        (
            "has_agent_resolution",
            RestrictedExpression::new_bool(change.resolved_by_agent()),
        ),
        ("label_names", label_names),
        ("owner_sets", long(review.owner_sets.into())),
        (
            "owner_sets_missing",
            long(review.missing_owner_sets.len() as i64),
        ),
        (
            "owners_covered",
            RestrictedExpression::new_bool(review.owners_covered()),
        ),
        (
            "intent_approved",
            RestrictedExpression::new_bool(change.intent.as_ref().is_some_and(|i| i.approved)),
        ),
        (
            "intent_in_scope",
            RestrictedExpression::new_bool(change.intent.as_ref().is_some_and(|i| i.in_scope)),
        ),
        (
            "independent_agent_approvals",
            long(review.independent_agent_approvals.into()),
        ),
        ("min_agent_trust", long(review.min_agent_trust.into())),
        ("files", long(change.total_files as i64)),
        ("lines", long(change.total_lines as i64)),
        ("stack_depth", long(change.stack_depth.into())),
        (
            "author_model",
            string(change.provenance.model.as_deref().unwrap_or("")),
        ),
        (
            "author_sponsor",
            string(
                &change
                    .provenance
                    .sponsor
                    .as_ref()
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
            ),
        ),
        ("weekend", RestrictedExpression::new_bool(change.weekend())),
        ("hour", long(change.hour().into())),
        ("trunk_state", string(&change.trunk_state)),
        ("frozen", RestrictedExpression::new_bool(change.frozen)),
        (
            "intent_agent_quorum",
            long(
                change
                    .intent
                    .as_ref()
                    .and_then(|i| i.agent_quorum)
                    .unwrap_or(0)
                    .into(),
            ),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v));
    let mut tags = Vec::with_capacity(review.labels.len());
    for (name, summary) in &review.labels {
        tags.push((name.clone(), vote_summary(summary)?));
    }
    let entity = Entity::new_with_tags(id.clone(), attrs, [], tags)
        .map_err(|e| Error::PolicyEvaluation(e.to_string()))?;
    Ok((id, entity))
}

/// Build the request and entity set for one evaluation, validated against
/// the schema.
pub fn build_request(
    principal: &Principal,
    change: &ChangeFacts,
    review: &SubmitFacts,
) -> Result<(Request, Entities)> {
    let (principal_uid, principal_entity) = principal_entity(principal)?;
    let (change_uid, change_entity) = change_entity(change, review)?;
    let entities = Entities::from_entities([principal_entity, change_entity], Some(schema()))
        .map_err(|e| Error::PolicyEvaluation(e.to_string()))?;
    let request = Request::new(
        principal_uid,
        land_action()?,
        change_uid,
        Context::empty(),
        Some(schema()),
    )
    .map_err(|e| Error::PolicyEvaluation(e.to_string()))?;
    Ok((request, entities))
}
