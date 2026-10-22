use proptest::prelude::*;

use super::content::{content_ref, object_hash};
use super::merge::merge_of;
use super::paths::path;
use crate::objects::change::FileState;
use crate::{
    ChangeId, ChangeVersion, FileChange, FileKind, IntentId, Principal, Provenance, Timestamp,
};

fn file_kind() -> impl Strategy<Value = FileKind> {
    prop_oneof![
        Just(FileKind::Regular),
        Just(FileKind::Executable),
        Just(FileKind::Symlink)
    ]
}

/// A file's state on one side of a merge; `None` is absent.
pub fn file_state() -> impl Strategy<Value = FileState> {
    prop::option::of((content_ref(), file_kind()))
}

/// Any file change, including conflicts.
pub fn file_change() -> impl Strategy<Value = FileChange> {
    prop_oneof![
        3 => (content_ref(), file_kind()).prop_map(|(content, kind)| FileChange::Set {
            content,
            kind,
            copy_from: None,
        }),
        1 => Just(FileChange::Delete),
        1 => merge_of(file_state(), 3).prop_map(FileChange::Conflict),
    ]
}

/// A structurally valid change version (its objects need not exist).
pub fn change_version() -> impl Strategy<Value = ChangeVersion> {
    (
        any::<[u8; 16]>(),
        prop::collection::vec(object_hash(), 0..3),
        prop::option::of(object_hash()),
        prop::collection::btree_map(path(), file_change(), 0..6),
        prop_oneof![
            Just(Principal::human("alice")),
            Just(Principal::agent("bot"))
        ],
        prop::option::of("[a-z-]{1,10}"),
        prop::option::of(any::<[u8; 16]>()),
        ".{0,40}",
        0i64..4_000_000_000_000,
    )
        .prop_map(
            |(id, parents, predecessor, file_changes, author, model, intent, message, at)| {
                ChangeVersion {
                    change_id: ChangeId::from_bytes(id),
                    parents,
                    predecessor,
                    subtree_changes: Default::default(),
                    file_changes,
                    author,
                    provenance: Provenance {
                        model,
                        ..Provenance::default()
                    },
                    intent: intent.map(IntentId::from_bytes),
                    message,
                    created_at: Timestamp::from_millis(at),
                    extra: Default::default(),
                }
            },
        )
}
