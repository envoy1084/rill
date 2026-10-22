//! Every event kind.

use rill_types::*;

use super::fixtures::{h, p, wire};
use crate::rill::v1 as pb;

#[test]
fn every_event_kind_roundtrips() {
    let change = ChangeId::new();
    let kinds = vec![
        EventKind::RepoCreated { name: "r".into() },
        EventKind::RefMoved {
            name: "feature".into(),
            old: None,
            new: Some(h("a")),
        },
        EventKind::ReviewSubmitted {
            change,
            version: h("a"),
        },
        EventKind::Voted {
            change,
            label: "Code-Review".into(),
            value: -2,
            version: h("a"),
        },
        EventKind::Commented {
            change,
            thread: "t1".into(),
            new_thread: true,
        },
        EventKind::ThreadResolved {
            change,
            thread: "t1".into(),
            resolved: false,
        },
        EventKind::StackSet {
            change,
            stack: vec![change, ChangeId::new()],
        },
        EventKind::WorkspaceChanged {
            workspace: WorkspaceId::new(),
            operation: OperationId::new(),
        },
        EventKind::WorkspaceDeleted {
            workspace: WorkspaceId::new(),
        },
        EventKind::IntentChanged {
            intent: IntentId::new(),
        },
        EventKind::RelationChanged {
            object: "repo".into(),
            relation: "reader".into(),
            subject: "*".into(),
            added: true,
        },
        EventKind::PolicyChanged {
            name: "submit".into(),
        },
        EventKind::QuarantineChanged {
            target: "test:core::racy".into(),
            quarantined: true,
        },
        EventKind::IncidentChanged {
            incident: "inc_1".into(),
            culprit: TrunkSeq(9),
            state: "open".into(),
        },
        EventKind::TrunkHealthChanged {
            state: "guarded".into(),
            frozen: true,
        },
        EventKind::RevertsRequested {
            selector: "session=s1".into(),
            count: 3,
        },
        EventKind::VersionStored {
            version: h("b"),
            change,
        },
        EventKind::StatusChanged {
            change,
            from: ReviewStatus::Open,
            to: ReviewStatus::Landable,
        },
        EventKind::Landed {
            seq: TrunkSeq(7),
            version: h("b"),
            change,
        },
        EventKind::LandRequested {
            ticket: LandTicket(u64::MAX),
            change,
            lane: Lane::Emergency,
        },
    ];
    let statuses = [
        LandStatus::Queued,
        LandStatus::Verifying,
        LandStatus::Landing,
        LandStatus::Landed { seq: TrunkSeq(3) },
        LandStatus::Failed {
            reason: "red".into(),
        },
        LandStatus::Conflict {
            paths: vec![p("src/a.rs")],
        },
        LandStatus::Returned { reason: "".into() },
        LandStatus::Ejected {
            reason: "votes withdrawn".into(),
        },
    ];
    let kinds =
        kinds.into_iter().chain(
            statuses
                .into_iter()
                .map(|status| EventKind::LandStatusChanged {
                    ticket: LandTicket(9),
                    change,
                    status,
                }),
        );
    for kind in kinds {
        let event = Event {
            kind,
            actor: Principal::agent("bot"),
            at: Timestamp::from_millis(42),
        };
        assert_eq!(
            Event::try_from(wire(pb::Event::from(&event))).unwrap(),
            event
        );
    }
}
