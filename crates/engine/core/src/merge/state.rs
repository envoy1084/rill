//! Merging file states: the algebra first, then text.

use bytes::Bytes;
use rill_store::ObjectStore;
use rill_types::objects::change::FileState;
use rill_types::{ContentRef, FileChange, FileKind, Merge};

use super::content::Structural;
use super::file::{merge_terms, resolved_bytes};
use crate::blobs::{store_content, BlobReader};
use crate::diff::is_binary;
use crate::error::Result;

/// Merge the states of one path across all sides.
///
/// Trivial resolution through the [`Merge`] algebra comes first and needs
/// no content. Otherwise, when every term is a present file of the same
/// non-symlink kind and none is binary, the contents are text-merged; a
/// clean result is stored through `objects` and returned as a `Set`.
/// Anything else is returned as a `Conflict` carrying the simplified terms.
pub async fn merge_file_state(
    terms: Merge<FileState>,
    reader: &dyn BlobReader,
    objects: &dyn ObjectStore,
) -> Result<FileChange> {
    Ok(merge_file_state_with(terms, reader, objects, None)
        .await?
        .change)
}

/// A merged path, and whether the structural merger resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub change: FileChange,
    pub structural: bool,
}

impl From<FileChange> for Merged {
    fn from(change: FileChange) -> Self {
        Merged {
            change,
            structural: false,
        }
    }
}

/// [`merge_file_state`], handing a three-way text conflict to
/// `structural` before giving up on it.
pub async fn merge_file_state_with(
    terms: Merge<FileState>,
    reader: &dyn BlobReader,
    objects: &dyn ObjectStore,
    structural: Option<Structural<'_>>,
) -> Result<Merged> {
    let terms = terms.simplify();
    if let Some(state) = terms.resolve_trivial() {
        return Ok(to_change(state.clone()).into());
    }
    if !terms.is_present_everywhere() {
        return Ok(FileChange::Conflict(terms).into());
    }
    let kinds: Vec<FileKind> = terms
        .iter()
        .map(|t| t.as_ref().map(|(_, k)| *k).unwrap_or(FileKind::Regular))
        .collect();
    let kind = kinds[0];
    if kind == FileKind::Symlink || kinds.iter().any(|k| *k != kind) {
        return Ok(FileChange::Conflict(terms).into());
    }
    let mut contents = Vec::with_capacity(terms.as_slice().len());
    for term in terms.iter() {
        // Present everywhere was checked above.
        let Some((content, _)) = term else {
            unreachable!()
        };
        let bytes = reader.read(content).await?;
        contents.push(bytes);
    }
    if contents.iter().any(|b| is_binary(b)) {
        return Ok(FileChange::Conflict(terms).into());
    }
    // Same shape as `terms`, so `from_vec` cannot fail.
    let text_terms = Merge::from_vec(contents).expect("same shape as terms");
    let hunks = merge_terms(&text_terms);
    if let Some(bytes) = resolved_bytes(&hunks) {
        let content = store_content(objects, Bytes::from(bytes)).await?;
        return Ok(FileChange::set(content, kind).into());
    }
    // Only a plain three-way conflict ([ours, base, theirs]) goes further.
    if let (Some(structural), [ours, base, theirs]) = (structural, text_terms.as_slice()) {
        if let Some(bytes) = structural
            .resolve(base.clone(), ours.clone(), theirs.clone())
            .await
        {
            let content = store_content(objects, Bytes::from(bytes)).await?;
            return Ok(Merged {
                change: FileChange::set(content, kind),
                structural: true,
            });
        }
    }
    Ok(FileChange::Conflict(terms).into())
}

/// Merge the states of one path given explicit removes and adds.
pub async fn merge_states(
    removes: Vec<FileState>,
    adds: Vec<FileState>,
    reader: &dyn BlobReader,
    objects: &dyn ObjectStore,
) -> Result<FileChange> {
    merge_file_state(Merge::from_removes_adds(removes, adds), reader, objects).await
}

/// Convert a resolved state into the change that produces it.
pub fn to_change(state: FileState) -> FileChange {
    match state {
        Some((content, kind)) => FileChange::set(content, kind),
        None => FileChange::Delete,
    }
}

/// The merge terms of a change: a `Conflict` contributes its terms, a
/// `Set` or `Delete` the single state it produces.
pub fn change_terms(change: &FileChange) -> Merge<FileState> {
    match change {
        FileChange::Conflict(terms) => terms.clone(),
        other => Merge::resolved(other.resolved_state().unwrap_or(None)),
    }
}

/// Inline content reference for small test and helper inputs.
pub fn inline(bytes: &[u8]) -> ContentRef {
    ContentRef::inline(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn st(s: &str) -> FileState {
        Some((inline(s.as_bytes()), FileKind::Regular))
    }

    async fn run(base: FileState, left: FileState, right: FileState) -> FileChange {
        let objects: Arc<dyn ObjectStore> = Arc::new(rill_store::MemoryObjectStore::new());
        merge_file_state(
            Merge::three_way(base, left, right),
            &objects,
            objects.as_ref(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn trivial_cases_skip_content() {
        assert_eq!(
            run(st("a"), st("a"), st("b")).await,
            FileChange::set(inline(b"b"), FileKind::Regular)
        );
        assert_eq!(run(st("a"), None, st("a")).await, FileChange::Delete);
        assert!(run(st("a"), None, st("b")).await.is_conflict());
    }

    #[tokio::test]
    async fn text_merge_and_conflict() {
        let merged = run(st("a\nb\nc\n"), st("A\nb\nc\n"), st("a\nb\nC\n")).await;
        assert_eq!(
            merged,
            FileChange::set(inline(b"A\nb\nC\n"), FileKind::Regular)
        );
        let conflict = run(st("a\n"), st("L\n"), st("R\n")).await;
        match conflict {
            FileChange::Conflict(terms) => assert_eq!(terms.num_sides(), 2),
            other => panic!("expected conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn binary_and_kind_mismatch_conflict() {
        assert!(run(st("a\0"), st("b\0"), st("c\0")).await.is_conflict());
        let exec = Some((inline(b"x\n"), FileKind::Executable));
        assert!(run(st("a\n"), st("b\n"), exec).await.is_conflict());
    }
}
