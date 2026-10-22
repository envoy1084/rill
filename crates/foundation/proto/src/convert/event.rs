//! Repository events.

use rill_types::{Event, EventKind, TrunkSeq};

use crate::convert::land::{
    land_status_from_parts, land_status_parts, lane_from_proto, ticket_from_proto,
};
use crate::convert::primitives::{
    hash_from_proto, hash_to_proto, id_from_proto, id_to_proto, ids_from_proto,
    opt_hash_from_proto, required, timestamp_from_proto, timestamp_to_proto, trunk_seq_from_proto,
    trunk_seq_to_proto,
};
use crate::convert::review::{i8_from_proto, review_status_from_proto};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<&EventKind> for pb::event::Kind {
    fn from(kind: &EventKind) -> Self {
        use pb::event::Kind;
        match kind {
            EventKind::RepoCreated { name } => {
                Kind::RepoCreated(pb::RepoCreatedEvent { name: name.clone() })
            }
            EventKind::RefMoved { name, old, new } => Kind::RefMoved(pb::RefMovedEvent {
                name: name.clone(),
                old: old.as_ref().map(hash_to_proto),
                new: new.as_ref().map(hash_to_proto),
            }),
            EventKind::ReviewSubmitted { change, version } => {
                Kind::ReviewSubmitted(pb::ReviewSubmittedEvent {
                    change_id: id_to_proto(change),
                    version: hash_to_proto(version),
                })
            }
            EventKind::Voted {
                change,
                label,
                value,
                version,
            } => Kind::Voted(pb::VotedEvent {
                change_id: id_to_proto(change),
                label: label.clone(),
                value: i32::from(*value),
                version: hash_to_proto(version),
            }),
            EventKind::Commented {
                change,
                thread,
                new_thread,
            } => Kind::Commented(pb::CommentedEvent {
                change_id: id_to_proto(change),
                thread_id: thread.clone(),
                new_thread: *new_thread,
            }),
            EventKind::ThreadResolved {
                change,
                thread,
                resolved,
            } => Kind::ThreadResolved(pb::ThreadResolvedEvent {
                change_id: id_to_proto(change),
                thread_id: thread.clone(),
                resolved: *resolved,
            }),
            EventKind::StackSet { change, stack } => Kind::StackSet(pb::StackSetEvent {
                change_id: id_to_proto(change),
                stack: stack.iter().map(id_to_proto).collect(),
            }),
            EventKind::WorkspaceChanged {
                workspace,
                operation,
            } => Kind::WorkspaceChanged(pb::WorkspaceChangedEvent {
                workspace_id: id_to_proto(workspace),
                operation_id: id_to_proto(operation),
            }),
            EventKind::WorkspaceDeleted { workspace } => {
                Kind::WorkspaceDeleted(pb::WorkspaceDeletedEvent {
                    workspace_id: id_to_proto(workspace),
                })
            }
            EventKind::IntentChanged { intent } => Kind::IntentChanged(pb::IntentChangedEvent {
                intent_id: id_to_proto(intent),
            }),
            EventKind::RelationChanged {
                object,
                relation,
                subject,
                added,
            } => Kind::RelationChanged(pb::RelationChangedEvent {
                object: object.clone(),
                relation: relation.clone(),
                subject: subject.clone(),
                added: *added,
            }),
            EventKind::PolicyChanged { name } => {
                Kind::PolicyChanged(pb::PolicyChangedEvent { name: name.clone() })
            }
            EventKind::QuarantineChanged {
                target,
                quarantined,
            } => Kind::QuarantineChanged(pb::QuarantineChangedEvent {
                target: target.clone(),
                quarantined: *quarantined,
            }),
            EventKind::IncidentChanged {
                incident,
                culprit,
                state,
            } => Kind::IncidentChanged(pb::IncidentChangedEvent {
                incident: incident.clone(),
                culprit: culprit.get(),
                state: state.clone(),
            }),
            EventKind::TrunkHealthChanged { state, frozen } => {
                Kind::TrunkHealthChanged(pb::TrunkHealthChangedEvent {
                    state: state.clone(),
                    frozen: *frozen,
                })
            }
            EventKind::RevertsRequested { selector, count } => {
                Kind::RevertsRequested(pb::RevertsRequestedEvent {
                    selector: selector.clone(),
                    count: *count,
                })
            }
            EventKind::VersionStored { version, change } => {
                Kind::VersionStored(pb::VersionStoredEvent {
                    version: hash_to_proto(version),
                    change_id: id_to_proto(change),
                })
            }
            EventKind::StatusChanged { change, from, to } => {
                Kind::StatusChanged(pb::StatusChangedEvent {
                    change_id: id_to_proto(change),
                    from: pb::ReviewStatus::from(*from) as i32,
                    to: pb::ReviewStatus::from(*to) as i32,
                })
            }
            EventKind::LandRequested {
                ticket,
                change,
                lane,
            } => Kind::LandRequested(pb::LandRequestedEvent {
                ticket: ticket.0.to_string(),
                change_id: id_to_proto(change),
                lane: lane.name().to_string(),
            }),
            EventKind::LandStatusChanged {
                ticket,
                change,
                status,
            } => {
                let (status, reason, seq, conflict_paths) = land_status_parts(status);
                Kind::LandStatusChanged(pb::LandStatusChangedEvent {
                    ticket: ticket.0.to_string(),
                    change_id: id_to_proto(change),
                    status,
                    reason,
                    seq,
                    conflict_paths,
                })
            }
            EventKind::Landed {
                seq,
                version,
                change,
            } => Kind::Landed(pb::LandedEvent {
                seq: trunk_seq_to_proto(*seq),
                version: hash_to_proto(version),
                change_id: id_to_proto(change),
            }),
        }
    }
}

impl TryFrom<pb::event::Kind> for EventKind {
    type Error = ConvertError;

    fn try_from(kind: pb::event::Kind) -> Result<Self> {
        use pb::event::Kind;
        Ok(match kind {
            Kind::RepoCreated(e) => EventKind::RepoCreated { name: e.name },
            Kind::RefMoved(e) => EventKind::RefMoved {
                name: e.name,
                old: opt_hash_from_proto("ref_moved.old", e.old.as_ref())?,
                new: opt_hash_from_proto("ref_moved.new", e.new.as_ref())?,
            },
            Kind::ReviewSubmitted(e) => EventKind::ReviewSubmitted {
                change: id_from_proto("review_submitted.change_id", &e.change_id)?,
                version: hash_from_proto("review_submitted.version", &e.version)?,
            },
            Kind::Voted(e) => EventKind::Voted {
                change: id_from_proto("voted.change_id", &e.change_id)?,
                label: e.label,
                value: i8_from_proto("voted.value", e.value)?,
                version: hash_from_proto("voted.version", &e.version)?,
            },
            Kind::Commented(e) => EventKind::Commented {
                change: id_from_proto("commented.change_id", &e.change_id)?,
                thread: e.thread_id,
                new_thread: e.new_thread,
            },
            Kind::ThreadResolved(e) => EventKind::ThreadResolved {
                change: id_from_proto("thread_resolved.change_id", &e.change_id)?,
                thread: e.thread_id,
                resolved: e.resolved,
            },
            Kind::StackSet(e) => EventKind::StackSet {
                change: id_from_proto("stack_set.change_id", &e.change_id)?,
                stack: ids_from_proto("stack_set.stack", &e.stack)?,
            },
            Kind::WorkspaceChanged(e) => EventKind::WorkspaceChanged {
                workspace: id_from_proto("workspace_changed.workspace_id", &e.workspace_id)?,
                operation: id_from_proto("workspace_changed.operation_id", &e.operation_id)?,
            },
            Kind::WorkspaceDeleted(e) => EventKind::WorkspaceDeleted {
                workspace: id_from_proto("workspace_deleted.workspace_id", &e.workspace_id)?,
            },
            Kind::IntentChanged(e) => EventKind::IntentChanged {
                intent: id_from_proto("intent_changed.intent_id", &e.intent_id)?,
            },
            Kind::RelationChanged(e) => EventKind::RelationChanged {
                object: e.object,
                relation: e.relation,
                subject: e.subject,
                added: e.added,
            },
            Kind::PolicyChanged(e) => EventKind::PolicyChanged { name: e.name },
            Kind::QuarantineChanged(e) => EventKind::QuarantineChanged {
                target: e.target,
                quarantined: e.quarantined,
            },
            Kind::IncidentChanged(e) => EventKind::IncidentChanged {
                incident: e.incident,
                culprit: TrunkSeq(e.culprit),
                state: e.state,
            },
            Kind::TrunkHealthChanged(e) => EventKind::TrunkHealthChanged {
                state: e.state,
                frozen: e.frozen,
            },
            Kind::RevertsRequested(e) => EventKind::RevertsRequested {
                selector: e.selector,
                count: e.count,
            },
            Kind::VersionStored(e) => EventKind::VersionStored {
                version: hash_from_proto("version_stored.version", &e.version)?,
                change: id_from_proto("version_stored.change_id", &e.change_id)?,
            },
            Kind::StatusChanged(e) => EventKind::StatusChanged {
                change: id_from_proto("status_changed.change_id", &e.change_id)?,
                from: review_status_from_proto("status_changed.from", e.from)?,
                to: review_status_from_proto("status_changed.to", e.to)?,
            },
            Kind::LandRequested(e) => EventKind::LandRequested {
                ticket: ticket_from_proto("land_requested.ticket", &e.ticket)?,
                change: id_from_proto("land_requested.change_id", &e.change_id)?,
                lane: lane_from_proto("land_requested.lane", &e.lane)?,
            },
            Kind::LandStatusChanged(e) => EventKind::LandStatusChanged {
                ticket: ticket_from_proto("land_status_changed.ticket", &e.ticket)?,
                change: id_from_proto("land_status_changed.change_id", &e.change_id)?,
                status: land_status_from_parts(
                    "land_status_changed.status",
                    &e.status,
                    e.reason,
                    e.seq,
                    &e.conflict_paths,
                )?,
            },
            Kind::Landed(e) => EventKind::Landed {
                seq: trunk_seq_from_proto(e.seq),
                version: hash_from_proto("landed.version", &e.version)?,
                change: id_from_proto("landed.change_id", &e.change_id)?,
            },
        })
    }
}

impl From<&Event> for pb::Event {
    fn from(e: &Event) -> Self {
        pb::Event {
            actor: Some(pb::Principal::from(&e.actor)),
            at: timestamp_to_proto(e.at),
            kind: Some(pb::event::Kind::from(&e.kind)),
        }
    }
}

impl From<Event> for pb::Event {
    fn from(e: Event) -> Self {
        pb::Event::from(&e)
    }
}

impl TryFrom<pb::Event> for Event {
    type Error = ConvertError;

    fn try_from(e: pb::Event) -> Result<Self> {
        Ok(Event {
            kind: required("event.kind", e.kind)?.try_into()?,
            actor: required("event.actor", e.actor)?.try_into()?,
            at: timestamp_from_proto(e.at),
        })
    }
}
