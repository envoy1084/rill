//! Rules a change must pass before it is stored, beyond path syntax.

use std::sync::Arc;

use rill_core::tree::{Edit, TreeBuilder, TreeOps};
use rill_core::ChangeBuilder;
use rill_store::MemoryObjectStore;
use rill_types::{ContentRef, FileKind, RepoPathBuf, Tree};

use crate::common::fixture;

#[tokio::test]
async fn symlinks_stored_as_blobs_cannot_escape_the_repository() {
    let f = fixture().await;
    // Longer than the inline threshold, so the target is read back from a
    // blob rather than checked from the change alone.
    let escape = format!("{}../../../etc/passwd", "./".repeat(200));
    let err = ChangeBuilder::new(f.alice.clone())
        .file_kind("deep/link", escape, FileKind::Symlink)
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("escapes the repository"), "{err}");

    let inside = format!("{}../target", "./".repeat(200));
    ChangeBuilder::new(f.alice.clone())
        .file_kind("deep/link", inside, FileKind::Symlink)
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
}

#[tokio::test]
async fn only_names_a_change_adds_are_checked_for_case_collisions() {
    let ops = TreeOps::new(Arc::new(MemoryObjectStore::new()), 16);
    let file =
        |s: &'static str| Edit::Set(ContentRef::inline(s.as_bytes().to_vec()), FileKind::Regular);
    // A directory that already holds a collision, as an old import might.
    let mut legacy = Tree::empty();
    legacy.insert_file("Make", ContentRef::inline(b"1".to_vec()), FileKind::Regular);
    legacy.insert_file("make", ContentRef::inline(b"2".to_vec()), FileKind::Regular);
    let base = ops.put_tree(&legacy).await.unwrap();
    let path = |p: &str| RepoPathBuf::new(p).unwrap();

    let mut unrelated = TreeBuilder::new();
    unrelated.edit(&path("other"), file("x"));
    unrelated.edit(&path("make"), file("3"));
    unrelated.build(&ops, Some(base)).await.unwrap();

    let mut colliding = TreeBuilder::new();
    colliding.edit(&path("OTHER"), file("x"));
    colliding.edit(&path("other"), file("y"));
    let err = colliding.build(&ops, Some(base)).await.unwrap_err();
    assert!(err.to_string().contains("differ only in case"), "{err}");
}
