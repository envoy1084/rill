//! Wire values back into daemon types, for clients.

use rill_types::RepoPathBuf;

use crate::overlay::{ChangeKind, LocalChange};
use crate::proto as pb;

/// The local changes of a status reply; unknown kinds are skipped.
pub fn local_changes(changes: &[pb::LocalChange]) -> Vec<LocalChange> {
    changes
        .iter()
        .filter_map(|c| {
            let kind = match c.kind.as_known()? {
                pb::ChangeKind::Added => ChangeKind::Added,
                pb::ChangeKind::Modified => ChangeKind::Modified,
                pb::ChangeKind::Deleted => ChangeKind::Deleted,
                pb::ChangeKind::Conflict => ChangeKind::Conflict,
                _ => return None,
            };
            Some(LocalChange {
                path: RepoPathBuf::new(&c.path).ok()?,
                kind,
            })
        })
        .collect()
}
