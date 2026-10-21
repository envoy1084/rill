//! What each logged change means to an auditor. Pure: the logs already
//! record every change atomically with the change itself, so the audit
//! log is built from them rather than written by every handler.

use std::collections::BTreeMap;

use rill_types::audit::{AuditOutcome, AuditRecord, INSTANCE_AUDIT};
use rill_types::identity::{IdentityChange, IdentityEvent};
use rill_types::{Event, EventKind, LandStatus, Principal, Timestamp};

fn record(
    org: &str,
    at: Timestamp,
    actor: &Principal,
    action: &str,
    target: String,
    detail: &[(&str, String)],
) -> AuditRecord {
    AuditRecord {
        org: org.to_string(),
        at,
        actor: actor.clone(),
        action: action.to_string(),
        target,
        outcome: AuditOutcome::Allowed,
        detail: detail
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// The audit record of an identity log entry.
pub fn identity(event: &IdentityEvent) -> AuditRecord {
    let (at, actor) = (event.at, &event.actor);
    let mine = |org: &str, action: &str, target: String, detail: &[(&str, String)]| {
        record(org, at, actor, action, target, detail)
    };
    match &event.change {
        IdentityChange::KeysChanged => {
            mine(INSTANCE_AUDIT, "keys.change", "token-keys".into(), &[])
        }
        IdentityChange::TokenRevoked { token_id, .. } => mine(
            INSTANCE_AUDIT,
            "token.revoke",
            format!("token:{token_id}"),
            &[],
        ),
        IdentityChange::SessionRevoked { session } => mine(
            INSTANCE_AUDIT,
            "session.revoke",
            format!("session:{session}"),
            &[],
        ),
        IdentityChange::UserChanged { org, name } => {
            mine(org, "user.change", format!("user:{name}"), &[])
        }
        IdentityChange::ProviderChanged { org, name } => {
            mine(org, "idp.change", format!("idp:{name}"), &[])
        }
        IdentityChange::AgentChanged { org, name } => {
            mine(org, "agent.change", format!("agent:{name}"), &[])
        }
        IdentityChange::WorkloadIssuerChanged { org, name } => {
            mine(org, "trust.change", format!("trust:{name}"), &[])
        }
        IdentityChange::TeamChanged { org, name } => {
            mine(org, "team.change", format!("team:{name}"), &[])
        }
        IdentityChange::OrgSettingsChanged { org } => {
            mine(org, "org.settings", format!("org:{org}"), &[])
        }
        IdentityChange::SessionOpened {
            org,
            principal,
            session,
            kind,
        } => mine(
            org,
            "session.open",
            principal.to_string(),
            &[("session", session.to_string()), ("kind", kind.clone())],
        ),
        IdentityChange::TokenIssued {
            org,
            principal,
            token_id,
            expires_at,
            scope,
        } => {
            let mut detail = vec![
                ("token", token_id.to_string()),
                ("expires_at", expires_at.to_string()),
            ];
            if let Some(scope) = scope {
                detail.push(("scope", scope.clone()));
            }
            mine(org, "token.issue", principal.to_string(), &detail)
        }
        IdentityChange::OrgCreated { org } => {
            mine(INSTANCE_AUDIT, "org.create", format!("org:{org}"), &[])
        }
        IdentityChange::RedactionRequested { org, id, hash } => mine(
            org,
            "redaction.request",
            format!("object:{hash}"),
            &[("redaction", id.clone())],
        ),
        IdentityChange::RedactionApplied { org, id, hash } => mine(
            org,
            "redaction.apply",
            format!("object:{hash}"),
            &[("redaction", id.clone())],
        ),
        IdentityChange::QuarantineCleared { org, repo, version } => mine(
            org,
            "quarantine.clear",
            format!("repo:{repo}"),
            &[("version", version.to_string())],
        ),
        IdentityChange::ScimCredentialChanged { org, id, active } => mine(
            org,
            if *active {
                "scim.credential.create"
            } else {
                "scim.credential.revoke"
            },
            format!("scim-credential:{id}"),
            &[],
        ),
    }
}

/// The audit record of a repository event, for the events an auditor
/// cares about: access, policy, review decisions and what reached trunk.
pub fn repo(org: &str, repo: &str, event: &Event) -> Option<AuditRecord> {
    let target = format!("repo:{repo}");
    let at = |action: &str, detail: &[(&str, String)]| {
        Some(record(
            org,
            event.at,
            &event.actor,
            action,
            target.clone(),
            detail,
        ))
    };
    match &event.kind {
        EventKind::RepoCreated { .. } => at("repo.create", &[]),
        EventKind::RelationChanged {
            object,
            relation,
            subject,
            added,
        } => at(
            if *added {
                "relation.add"
            } else {
                "relation.remove"
            },
            &[
                ("object", object.clone()),
                ("relation", relation.clone()),
                ("subject", subject.clone()),
            ],
        ),
        EventKind::PolicyChanged { name } => at("policy.set", &[("policy", name.clone())]),
        EventKind::RefMoved { name, old, new } => at(
            "ref.move",
            &[
                ("ref", name.clone()),
                ("old", old.map(|h| h.to_string()).unwrap_or_default()),
                ("new", new.map(|h| h.to_string()).unwrap_or_default()),
            ],
        ),
        EventKind::ReviewSubmitted { change, version } => at(
            "review.submit",
            &[
                ("change", change.to_string()),
                ("version", version.to_string()),
            ],
        ),
        EventKind::Voted {
            change,
            label,
            value,
            version,
        } => at(
            "review.vote",
            &[
                ("change", change.to_string()),
                ("label", label.clone()),
                ("value", value.to_string()),
                ("version", version.to_string()),
            ],
        ),
        EventKind::LandRequested { change, lane, .. } => at(
            "land.request",
            &[
                ("change", change.to_string()),
                ("lane", format!("{lane:?}").to_lowercase()),
            ],
        ),
        EventKind::LandStatusChanged {
            change,
            status: LandStatus::Failed { reason },
            ..
        } => at(
            "land.refuse",
            &[("change", change.to_string()), ("reason", reason.clone())],
        ),
        EventKind::Landed {
            seq,
            version,
            change,
        } => at(
            "land",
            &[
                ("seq", seq.get().to_string()),
                ("version", version.to_string()),
                ("change", change.to_string()),
            ],
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_actions_go_to_the_instance_stream_and_org_actions_to_theirs() {
        let event = |change| IdentityEvent {
            change,
            actor: Principal::human("root"),
            at: Timestamp::from_millis(1),
        };
        assert_eq!(
            identity(&event(IdentityChange::KeysChanged)).org,
            INSTANCE_AUDIT
        );
        assert_eq!(
            identity(&event(IdentityChange::OrgCreated { org: "acme".into() })).org,
            INSTANCE_AUDIT
        );
        let user = identity(&event(IdentityChange::UserChanged {
            org: "acme".into(),
            name: "ann".into(),
        }));
        assert_eq!(
            (user.org.as_str(), user.target.as_str()),
            ("acme", "user:ann")
        );
        let routine = Event {
            kind: EventKind::WorkspaceDeleted {
                workspace: rill_types::WorkspaceId::new(),
            },
            actor: Principal::human("ann"),
            at: Timestamp::from_millis(1),
        };
        assert!(
            repo("acme", "web", &routine).is_none(),
            "routine work is not audited"
        );
    }
}
