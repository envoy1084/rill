//! Change versions, change info and signatures.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::objects::change::CopySource;
use rill_types::*;

use super::fixtures::{h, p, wire};
use crate::error::ConvertError;
use crate::rill::v1 as pb;

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
    wire_cv.provenance = None.into();
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
    bad.author = None.into();
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
                content: pb::ContentRef::from(ContentRef::inline(Vec::new())).into(),
                kind: 0.into(),
                copy_from: None.into(),
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
