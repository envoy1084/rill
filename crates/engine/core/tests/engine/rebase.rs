//! Rebasing changes that carry whole-directory (subtree) operations.

use crate::common::{content_at, fixture, t, Fixture};
use rill_core::{rebase, ChangeBuilder};
use rill_types::{ChangeVersionHash, SubtreeChange};

struct History {
    root: ChangeVersionHash,
    copy_change: ChangeVersionHash,
}

/// `root` has `lib/{a,b}.rs`; `copy_change` replaces `lib` with a copy
/// from a version where only `lib/a.rs` differs.
async fn history(f: &Fixture) -> History {
    let root = ChangeBuilder::new(f.alice.clone())
        .created_at(t(1))
        .file("lib/a.rs", "a\n")
        .unwrap()
        .file("lib/b.rs", "b\n")
        .unwrap()
        .file("src/x.rs", "x\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0;
    let source = ChangeBuilder::new(f.alice.clone())
        .parent(root)
        .created_at(t(2))
        .file("lib/a.rs", "a2\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0;
    let copy_change = ChangeBuilder::new(f.alice.clone())
        .parent(root)
        .created_at(t(3))
        .subtree(
            "lib",
            SubtreeChange::Copy {
                from_path: crate::common::p("lib"),
                from_version: source,
            },
        )
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0;
    History { root, copy_change }
}

async fn child(
    f: &Fixture,
    parent: ChangeVersionHash,
    path: &str,
    text: &str,
) -> ChangeVersionHash {
    ChangeBuilder::new(f.bob.clone())
        .parent(parent)
        .created_at(t(4))
        .file(path, text.to_string())
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn subtree_copy_does_not_clobber_new_parent_edits() {
    let f = fixture().await;
    let h = history(&f).await;
    let new_parent = child(&f, h.root, "lib/b.rs", "B\n").await;

    let rebased = rebase(&f.derive, &h.copy_change, &new_parent, t(5))
        .await
        .unwrap();
    assert!(rebased.subtree_changes.is_empty());
    let hash = f.repo.put_change_version(&rebased).await.unwrap();
    assert_eq!(
        content_at(&f, &hash, "lib/a.rs").await.unwrap(),
        "a2\n".as_bytes()
    );
    assert_eq!(
        content_at(&f, &hash, "lib/b.rs").await.unwrap(),
        "B\n".as_bytes()
    );
}

#[tokio::test]
async fn subtree_copy_is_kept_when_directory_is_untouched() {
    let f = fixture().await;
    let h = history(&f).await;
    let new_parent = child(&f, h.root, "src/x.rs", "X\n").await;

    let rebased = rebase(&f.derive, &h.copy_change, &new_parent, t(5))
        .await
        .unwrap();
    assert_eq!(rebased.subtree_changes.len(), 1);
    let hash = f.repo.put_change_version(&rebased).await.unwrap();
    assert_eq!(
        content_at(&f, &hash, "lib/a.rs").await.unwrap(),
        "a2\n".as_bytes()
    );
    assert_eq!(
        content_at(&f, &hash, "src/x.rs").await.unwrap(),
        "X\n".as_bytes()
    );
}
