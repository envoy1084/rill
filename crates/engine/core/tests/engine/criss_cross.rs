//! Merging across a criss-cross history, where two versions share more
//! than one best common ancestor.

use crate::common::{content_at, fixture, t, Fixture};
use rill_core::{merge_changes, ChangeBuilder, MergeChangeOptions};
use rill_types::{ChangeVersionHash, FileChange};

async fn commit(f: &Fixture, parent: ChangeVersionHash, at: i64, text: &str) -> ChangeVersionHash {
    ChangeBuilder::new(f.alice.clone())
        .parent(parent)
        .message(text.lines().collect::<Vec<_>>().join(" "))
        .created_at(t(at))
        .file("f", text.to_string())
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0
}

async fn merge(
    f: &Fixture,
    left: ChangeVersionHash,
    right: ChangeVersionHash,
    at: i64,
) -> ChangeVersionHash {
    let options = MergeChangeOptions {
        created_at: t(at),
        ..MergeChangeOptions::new(f.alice.clone(), "merge")
    };
    let merged = merge_changes(&f.derive, &f.graph, &left, &right, options)
        .await
        .unwrap();
    assert!(!merged.has_conflicts());
    f.repo.put_change_version(&merged).await.unwrap()
}

#[tokio::test]
async fn criss_cross_merge_uses_a_virtual_base() {
    let f = fixture().await;
    let root = ChangeBuilder::new(f.alice.clone())
        .created_at(t(1))
        .file("f", "a\nb\nc\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0;
    let x1 = commit(&f, root, 2, "A\nb\nc\n").await;
    let y1 = commit(&f, root, 3, "a\nb\nC\n").await;
    // Each side merges the other: x1 and y1 both become best common
    // ancestors of everything built on x2 and y2.
    let x2 = merge(&f, x1, y1, 4).await;
    let y2 = merge(&f, y1, x1, 5).await;
    let left = commit(&f, x2, 6, "A\nB\nC\n").await;
    let right = commit(&f, y2, 7, "A\nb\nC\nd\n").await;

    let mut bases = f.graph.merge_bases(&left, &right).await.unwrap();
    bases.sort();
    let mut expected = vec![x1, y1];
    expected.sort();
    assert_eq!(bases, expected);

    // Against either real base alone the edits overlap and conflict; the
    // virtual base (x1 merged with y1) separates them.
    let merged = merge(&f, left, right, 8).await;
    assert_eq!(
        content_at(&f, &merged, "f").await.unwrap(),
        "A\nB\nC\nd\n".as_bytes()
    );
}

#[tokio::test]
async fn conflicting_edits_still_conflict_across_criss_cross() {
    let f = fixture().await;
    let root = ChangeBuilder::new(f.alice.clone())
        .created_at(t(1))
        .file("f", "a\nb\nc\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0;
    let x1 = commit(&f, root, 2, "A\nb\nc\n").await;
    let y1 = commit(&f, root, 3, "a\nb\nC\n").await;
    let x2 = merge(&f, x1, y1, 4).await;
    let y2 = merge(&f, y1, x1, 5).await;
    let left = commit(&f, x2, 6, "A\nLEFT\nC\n").await;
    let right = commit(&f, y2, 7, "A\nRIGHT\nC\n").await;

    let options = MergeChangeOptions::new(f.alice.clone(), "merge");
    let merged = merge_changes(&f.derive, &f.graph, &left, &right, options)
        .await
        .unwrap();
    assert!(matches!(
        merged.file_changes.values().next(),
        Some(FileChange::Conflict(_))
    ));
}
