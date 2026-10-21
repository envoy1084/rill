//! Round-trip tests through both the conversion layer and prost encoding,
//! so that what goes over the wire decodes to an equal `rill-types` value.

use std::collections::BTreeMap;

use bytes::Bytes;
use prost::Message;
use rill_types::objects::change::CopySource;
use rill_types::objects::review::CommentCategory;
use rill_types::*;

use crate::error::ConvertError;
use crate::rill::v1 as pb;

fn h(s: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::ChangeVersion, s.as_bytes())
}

fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

/// Encode to bytes and decode again, so prost's own serialization is part of
/// the round trip.
fn wire<M: Message + Default>(m: M) -> M {
    M::decode(m.encode_to_vec().as_slice()).unwrap()
}

fn sample_change_version() -> ChangeVersion {
    let mut file_changes = BTreeMap::new();
    file_changes.insert(
        p("src/lib.rs"),
        FileChange::set(
            ContentRef::inline(b"fn main(){}".to_vec()),
            FileKind::Regular,
        ),
    );
    file_changes.insert(
        p("src/new.rs"),
        FileChange::Set {
            content: ContentRef::Blob {
                hash: h("blob"),
                size: 4096,
            },
            kind: FileKind::Executable,
            copy_from: Some(CopySource {
                path: p("src/old.rs"),
                version: h("parent"),
            }),
        },
    );
    file_changes.insert(p("src/old.rs"), FileChange::Delete);
    file_changes.insert(
        p("README.md"),
        FileChange::Conflict(Merge::three_way(
            Some((ContentRef::inline(b"base".to_vec()), FileKind::Regular)),
            Some((ContentRef::inline(b"ours".to_vec()), FileKind::Regular)),
            None,
        )),
    );
    let mut subtree_changes = BTreeMap::new();
    subtree_changes.insert(
        p("vendor/dep"),
        SubtreeChange::Copy {
            from_path: p("third_party/dep"),
            from_version: h("parent"),
        },
    );
    subtree_changes.insert(p("legacy"), SubtreeChange::Delete);
    let mut extra = BTreeMap::new();
    extra.insert("x".to_string(), Bytes::from_static(b"\x00\x01"));
    let mut provenance = Provenance {
        tool: Some("claude-code".into()),
        model: Some("claude".into()),
        session: Some("s1".into()),
        prompt_ref: None,
        sponsor: Some(Principal::human("alice")),
        resolved_by: Some("auto".into()),
        extra: BTreeMap::new(),
    };
    provenance.extra.insert("k".into(), "v".into());
    ChangeVersion {
        change_id: ChangeId::derived("test", "cv"),
        parents: vec![h("parent"), h("other")],
        predecessor: Some(h("pred")),
        subtree_changes,
        file_changes,
        author: Principal::agent("coder"),
        provenance,
        intent: Some(IntentId::derived("test", "intent")),
        message: "merge\n\nbody".into(),
        created_at: Timestamp::from_millis(1_700_000_000_000),
        extra,
    }
}

#[test]
fn change_version_roundtrip() {
    let cv = sample_change_version();
    let back = ChangeVersion::try_from(wire(pb::ChangeVersion::from(&cv))).unwrap();
    assert_eq!(back, cv);
    assert_eq!(back.hash(), cv.hash());
    assert!(back.has_conflicts());
}

#[test]
fn change_version_minimal_roundtrip() {
    let cv = ChangeVersion {
        change_id: ChangeId::derived("test", "root"),
        parents: vec![],
        predecessor: None,
        subtree_changes: BTreeMap::new(),
        file_changes: BTreeMap::new(),
        author: Principal::human("bob"),
        provenance: Provenance::default(),
        intent: None,
        message: String::new(),
        created_at: Timestamp::from_millis(0),
        extra: BTreeMap::new(),
    };
    let mut wire_cv = wire(pb::ChangeVersion::from(&cv));
    // Clients may omit provenance entirely.
    wire_cv.provenance = None;
    assert_eq!(ChangeVersion::try_from(wire_cv).unwrap(), cv);
}

#[test]
fn change_version_errors_name_fields() {
    let mut bad = pb::ChangeVersion::from(sample_change_version());
    bad.parents.push(Bytes::from_static(b"short"));
    assert_eq!(
        ChangeVersion::try_from(bad).unwrap_err(),
        ConvertError::BadLength {
            field: "change_version.parents",
            expected: 32,
            actual: 5
        }
    );

    let mut bad = pb::ChangeVersion::from(sample_change_version());
    bad.author = None;
    assert_eq!(
        ChangeVersion::try_from(bad).unwrap_err(),
        ConvertError::missing("change_version.author")
    );

    let mut bad = pb::ChangeVersion::from(sample_change_version());
    bad.file_changes
        .insert("../escape".into(), pb::FileChange::from(FileChange::Delete));
    assert!(matches!(
        ChangeVersion::try_from(bad).unwrap_err(),
        ConvertError::Invalid {
            field: "change_version.file_changes",
            ..
        }
    ));

    let mut bad = pb::ChangeVersion::from(sample_change_version());
    bad.file_changes.insert(
        "x".into(),
        pb::FileChange {
            change: Some(pb::file_change::Change::Set(pb::file_change::Set {
                content: Some(pb::ContentRef::from(ContentRef::inline(Vec::new()))),
                kind: 0,
                copy_from: None,
            })),
        },
    );
    assert_eq!(
        ChangeVersion::try_from(bad).unwrap_err(),
        ConvertError::BadEnum {
            field: "file_change.set.kind",
            value: 0
        }
    );
}

#[test]
fn change_info_roundtrip() {
    let cv = sample_change_version();
    let info = ChangeInfo::from_version(&cv, Some(TrunkSeq(42)));
    assert_eq!(
        ChangeInfo::try_from(wire(pb::ChangeInfo::from(&info))).unwrap(),
        info
    );
    let unlanded = ChangeInfo::from_version(&cv, None);
    assert_eq!(
        ChangeInfo::try_from(wire(pb::ChangeInfo::from(&unlanded))).unwrap(),
        unlanded
    );
}

#[test]
fn signature_roundtrip() {
    let sig = Signature {
        signer: Principal::service("lander"),
        public_key: Bytes::from(vec![1u8; 32]),
        signature: Bytes::from(vec![2u8; 64]),
        signed_at: Timestamp::from_millis(5),
    };
    assert_eq!(
        Signature::try_from(wire(pb::Signature::from(&sig))).unwrap(),
        sig
    );
}

#[test]
fn review_state_roundtrip() {
    let state = ReviewState {
        change_id: ChangeId::derived("test", "cv"),
        versions: vec![h("v1"), h("v2")],
        status: ReviewStatus::Landable,
        intent: Some(IntentId::derived("test", "i")),
        owner: Principal::human("alice"),
        votes: vec![LabelVote {
            label: "Review".into(),
            value: -2,
            voter: Principal::agent("reviewer"),
            version: h("v2"),
            voted_at: Timestamp::from_millis(10),
            reason: Some("nope".into()),
        }],
        threads: vec![
            Thread {
                id: "t1".into(),
                anchor: ThreadAnchor::Lines {
                    version: h("v2"),
                    path: p("src/lib.rs"),
                    start: 3,
                    end: 7,
                    context_hash: "abc".into(),
                },
                comments: vec![Comment {
                    author: Principal::agent("reviewer"),
                    body: "bug".into(),
                    created_at: Timestamp::from_millis(11),
                    category: Some(CommentCategory::Bug),
                    confidence: Some(87),
                }],
                resolved: false,
                to: Some(Principal::human("alice")),
            },
            Thread {
                id: "t2".into(),
                anchor: ThreadAnchor::Change,
                comments: vec![Comment {
                    author: Principal::human("alice"),
                    body: "lgtm".into(),
                    created_at: Timestamp::from_millis(12),
                    category: None,
                    confidence: None,
                }],
                resolved: true,
                to: None,
            },
            Thread {
                id: "t3".into(),
                anchor: ThreadAnchor::File {
                    version: h("v1"),
                    path: p("docs/x.md"),
                },
                comments: vec![],
                resolved: false,
                to: None,
            },
        ],
        risk: RiskClass::Sensitive,
        risk_reasons: vec!["touches auth".into()],
        stack: vec![
            ChangeId::derived("test", "a"),
            ChangeId::derived("test", "cv"),
        ],
        landed_seq: None,
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(2),
        extra: BTreeMap::from([("k".to_string(), "v".to_string())]),
    };
    assert_eq!(
        ReviewState::try_from(wire(pb::ReviewState::from(&state))).unwrap(),
        state
    );

    let mut bad = pb::ReviewState::from(&state);
    bad.votes[0].value = 1000;
    assert_eq!(
        ReviewState::try_from(bad).unwrap_err(),
        ConvertError::OutOfRange {
            field: "label_vote.value",
            value: 1000
        }
    );
    let mut bad = pb::ReviewState::from(&state);
    bad.threads[0].comments[0].confidence = Some(101);
    assert!(matches!(
        ReviewState::try_from(bad).unwrap_err(),
        ConvertError::OutOfRange {
            field: "comment.confidence",
            ..
        }
    ));
}

#[test]
fn label_roundtrip() {
    for label in [Label::review(), Label::verified(), Label::owner()] {
        assert_eq!(
            Label::try_from(wire(pb::Label::from(&label))).unwrap(),
            label
        );
    }
}

fn workspace_with(snapshot: Option<ObjectHash>) -> Workspace {
    Workspace {
        id: WorkspaceId::derived("test", "ws"),
        repo: RepoId::derived("test", "repo"),
        owner: Principal::agent("coder"),
        name: "feature-x".into(),
        base: h("base"),
        working_change: ChangeId::derived("test", "wc"),
        sparse: SparseProfile {
            include: vec![p("src")],
            exclude: vec![p("src/vendor")],
        },
        head_op: Some(OperationId::derived("test", "op")),
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(2),
        snapshot,
    }
}

#[test]
fn workspace_roundtrip() {
    let ws = workspace_with(Some(ObjectHash::digest(ObjectKind::Tree, b"t")));
    assert_eq!(
        Workspace::try_from(wire(pb::Workspace::from(&ws))).unwrap(),
        ws
    );

    let mut full = ws.clone();
    full.sparse = SparseProfile::full();
    full.snapshot = None;
    full.head_op = None;
    let mut w = wire(pb::Workspace::from(&full));
    w.sparse = None;
    assert_eq!(Workspace::try_from(w).unwrap(), full);
}

#[test]
fn operation_roundtrip_every_kind() {
    let kinds = vec![
        OperationKind::Create,
        OperationKind::Snapshot { tree: h("t") },
        OperationKind::NewChange {
            change: ChangeId::derived("test", "c"),
            base: h("b"),
        },
        OperationKind::Describe { version: h("v") },
        OperationKind::Rebase {
            from: h("a"),
            to: h("b"),
        },
        OperationKind::Resolve { version: h("v") },
        OperationKind::SetSparse {
            profile: SparseProfile {
                include: vec![p("a")],
                exclude: vec![],
            },
        },
        OperationKind::Handoff {
            to: Principal::human("bob"),
        },
        OperationKind::Undo {
            undone: OperationId::derived("test", "u"),
        },
        OperationKind::Other {
            description: "import".into(),
        },
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        let op = Operation {
            id: OperationId::derived("test", &i.to_string()),
            workspace: WorkspaceId::derived("test", "ws"),
            parent: (i > 0).then(|| OperationId::derived("test", &(i - 1).to_string())),
            actor: Principal::agent("coder"),
            kind,
            resulting: workspace_with((i % 2 == 0).then(|| h("snap"))),
            at: Timestamp::from_millis(i as i64),
        };
        assert_eq!(
            Operation::try_from(wire(pb::Operation::from(&op))).unwrap(),
            op
        );
    }

    let bad = pb::Operation {
        kind: None,
        ..pb::Operation::from(Operation {
            id: OperationId::derived("t", "x"),
            workspace: WorkspaceId::derived("t", "w"),
            parent: None,
            actor: Principal::human("a"),
            kind: OperationKind::Create,
            resulting: workspace_with(None),
            at: Timestamp::from_millis(0),
        })
    };
    assert_eq!(
        Operation::try_from(bad).unwrap_err(),
        ConvertError::missing("operation.kind")
    );
}

#[test]
fn tree_roundtrip_through_wire() {
    let mut tree = Tree::empty();
    tree.insert_file("a", ContentRef::inline(b"1".to_vec()), FileKind::Regular);
    tree.insert_file("link", ContentRef::inline(b"a".to_vec()), FileKind::Symlink);
    tree.insert_dir(
        "d",
        ObjectHash::digest(ObjectKind::Tree, b"d"),
        rill_types::objects::tree::DirSummary {
            file_count: 3,
            dir_count: 1,
            total_size: 99,
            max_depth: 2,
        },
    );
    let back = Tree::try_from(wire(pb::Tree::from(&tree))).unwrap();
    assert_eq!(back, tree);
}

#[test]
fn descriptor_set_is_embedded_and_parses() {
    let fds = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET).unwrap();
    let names: Vec<_> = fds.file.iter().map(|f| f.name().to_string()).collect();
    for expected in [
        "rill/v1/types.proto",
        "rill/v1/repo.proto",
        "rill/v1/workspace.proto",
        "rill/v1/review.proto",
        "rill/v1/intent.proto",
        "rill/v1/lander.proto",
        "rill/v1/admin.proto",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
    let services: Vec<_> = fds
        .file
        .iter()
        .flat_map(|f| f.service.iter().map(|s| s.name().to_string()))
        .collect();
    assert_eq!(services.len(), 6);
}

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
