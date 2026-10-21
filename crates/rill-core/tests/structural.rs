//! The structural merge hook: consulted only when the line merge conflicts
//! on a file it handles, recorded on the version, and a panic, a timeout
//! or a decline leaves the conflict.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use common::{fixture, p, t};
use rill_core::merge::{ContentMerger, STRUCTURAL_MERGE_EXTRA};
use rill_core::{rebase, ChangeBuilder};
use rill_types::{ChangeVersionHash, FileChange, RepoPath};

#[derive(Clone, Copy)]
enum Behaviour {
    Resolve,
    Decline,
    Panic,
    Slow,
}

struct Fake {
    behaviour: Behaviour,
    calls: AtomicUsize,
}

impl ContentMerger for Fake {
    fn name(&self) -> &str {
        "fake-1"
    }

    fn handles(&self, path: &RepoPath) -> bool {
        path.as_str().ends_with(".rs")
    }

    fn merge(&self, _: &RepoPath, _: &[u8], ours: &[u8], theirs: &[u8]) -> Option<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.behaviour {
            Behaviour::Resolve => Some([ours, theirs].concat()),
            Behaviour::Decline => None,
            Behaviour::Panic => panic!("merger bug"),
            Behaviour::Slow => {
                std::thread::sleep(Duration::from_millis(500));
                Some(Vec::new())
            }
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(100)
    }
}

/// Base `file` = "a\nz\n"; trunk and the change each add a line after
/// `a` (or, with `apart`, at opposite ends). Returns the change rebased on
/// trunk.
async fn rebased(
    file: &str,
    apart: bool,
    merger: Option<Arc<Fake>>,
) -> (rill_types::ChangeVersion, usize) {
    let f = fixture().await;
    if let Some(m) = &merger {
        f.derive.set_merger(m.clone());
    }
    let (base, _) = commit(&f, file, None, "a\nm\nz\n", 1).await;
    let (trunk, _) = commit(
        &f,
        file,
        Some(base),
        if apart {
            "t\na\nm\nz\n"
        } else {
            "a\nt\nm\nz\n"
        },
        2,
    )
    .await;
    let (change, _) = commit(
        &f,
        file,
        Some(base),
        if apart {
            "a\nm\nz\nc\n"
        } else {
            "a\nc\nm\nz\n"
        },
        3,
    )
    .await;
    let cv = rebase(&f.derive, &change, &trunk, t(4)).await.unwrap();
    let calls = merger.map_or(0, |m| m.calls.load(Ordering::SeqCst));
    (cv, calls)
}

async fn commit(
    f: &common::Fixture,
    file: &str,
    parent: Option<ChangeVersionHash>,
    body: &str,
    at: i64,
) -> (ChangeVersionHash, rill_types::ChangeVersion) {
    let mut builder = ChangeBuilder::new(f.alice.clone())
        .created_at(t(at))
        .message("m");
    if let Some(parent) = parent {
        builder = builder.parent(parent);
    }
    builder
        .file(file, body.to_string())
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap()
}

fn fake(behaviour: Behaviour) -> Arc<Fake> {
    Arc::new(Fake {
        behaviour,
        calls: AtomicUsize::new(0),
    })
}

#[tokio::test]
async fn a_conflict_the_merger_resolves_is_recorded_on_the_version() {
    let (cv, calls) = rebased("src/a.rs", false, Some(fake(Behaviour::Resolve))).await;
    assert_eq!(calls, 1);
    assert!(matches!(
        cv.file_changes.get(&p("src/a.rs")),
        Some(FileChange::Set { .. })
    ));
    assert_eq!(
        cv.extra.get(STRUCTURAL_MERGE_EXTRA).map(|b| b.as_ref()),
        Some(&b"fake-1: src/a.rs"[..])
    );
    let (plain, _) = rebased("src/a.rs", false, None).await;
    assert!(
        plain.file_changes[&p("src/a.rs")].is_conflict(),
        "without a merger it conflicts"
    );
}

#[tokio::test]
async fn the_merger_sees_only_conflicts_in_files_it_handles() {
    let (cv, calls) = rebased("src/a.rs", true, Some(fake(Behaviour::Resolve))).await;
    assert_eq!(calls, 0, "the line merge was clean");
    assert!(!cv.extra.contains_key(STRUCTURAL_MERGE_EXTRA));
    let (cv, calls) = rebased("notes.txt", false, Some(fake(Behaviour::Resolve))).await;
    assert_eq!(calls, 0);
    assert!(cv.file_changes[&p("notes.txt")].is_conflict());
}

#[tokio::test]
async fn a_decline_a_panic_or_a_timeout_leaves_the_conflict() {
    for behaviour in [Behaviour::Decline, Behaviour::Panic, Behaviour::Slow] {
        let (cv, calls) = rebased("src/a.rs", false, Some(fake(behaviour))).await;
        assert_eq!(calls, 1);
        assert!(cv.file_changes[&p("src/a.rs")].is_conflict());
        assert!(!cv.extra.contains_key(STRUCTURAL_MERGE_EXTRA));
    }
}
