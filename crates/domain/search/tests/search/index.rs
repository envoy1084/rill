//! Indexing: deltas cost what changed, other processes see a writer's
//! work, compaction keeps answers, a replaced writer is fenced off.

use rill_search::{IndexConfig, SearchIndex, SearchQuery};

use crate::common::*;

#[tokio::test]
async fn a_landing_indexes_only_its_changes_as_a_delta() {
    let f = fixture().await;
    let again = f.index.index_root(f.root, None).await.unwrap();
    assert!(again.already_indexed);
    let v2 = f
        .child(f.version, &[("src/main.rs", "fn main() { goodbye(); }\n")])
        .await;
    let (root2, stats) = f.index.index_version(&v2, None).await.unwrap();
    assert!(stats.delta);
    assert_eq!((stats.blobs_indexed, stats.files), (1, 4));
    let r = run(&f.index, root2, SearchQuery::new("goodbye"), all).await;
    assert_eq!(lines(&r.hits), [("src/main.rs".into(), 1)]);
    assert!(run(&f.index, f.root, SearchQuery::new("goodbye"), all)
        .await
        .hits
        .is_empty());
    assert!(run(&f.index, root2, SearchQuery::new("Hello, World"), all)
        .await
        .hits
        .is_empty());
}

#[tokio::test]
async fn another_process_reads_what_the_writer_indexed() {
    let f = fixture().await;
    let reader = SearchIndex::open(f.repo.clone(), f.derive.clone(), config())
        .await
        .unwrap();
    let v2 = f
        .child(f.version, &[("src/new.rs", "fn freshly_indexed() {}\n")])
        .await;
    let (root2, _) = f.index.index_version(&v2, None).await.unwrap();
    let r = run(&reader, root2, SearchQuery::new("freshly_indexed"), all).await;
    assert_eq!(lines(&r.hits), [("src/new.rs".into(), 1)]);
    let reopened = SearchIndex::open(f.repo.clone(), f.derive.clone(), config())
        .await
        .unwrap();
    let (_, stats) = reopened.index_version(&v2, None).await.unwrap();
    assert!(stats.already_indexed);
}

#[tokio::test]
async fn compaction_merges_segments_and_keeps_every_answer() {
    let f = fixture().await;
    let mut v = f.version;
    for n in 0..12 {
        v = f
            .child(
                v,
                &[(&format!("src/f{n}.rs"), &format!("fn unique_{n}() {{}}\n"))],
            )
            .await;
        f.index.index_version(&v, None).await.unwrap();
    }
    let before = f.index.snapshot().segments();
    let stats = f.index.compact(None).await.unwrap();
    assert!(
        stats.merges > 0 && stats.segments_after < before,
        "{stats:?}"
    );
    let root = f.derive.get::<rill_core::RootTree>(&v).await.unwrap();
    for n in [0, 5, 11] {
        let r = run(&f.index, root, SearchQuery::new(format!("unique_{n}")), all).await;
        assert_eq!(lines(&r.hits), [(format!("src/f{n}.rs"), 1)]);
    }
    let reader = SearchIndex::open(f.repo.clone(), f.derive.clone(), config())
        .await
        .unwrap();
    assert_eq!(reader.snapshot().segments(), stats.segments_after);
}

#[tokio::test]
async fn segments_can_live_in_a_local_cache_directory() {
    let f = fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let cached = SearchIndex::open(
        f.repo.clone(),
        f.derive.clone(),
        IndexConfig {
            cache_dir: Some(dir.path().to_path_buf()),
            ..config()
        },
    )
    .await
    .unwrap();
    let r = run(&cached, f.root, SearchQuery::new("Greeter"), all).await;
    assert_eq!(lines(&r.hits), [("py/lib.py".into(), 1)]);
    assert!(std::fs::read_dir(dir.path()).unwrap().count() > 0);
}

#[tokio::test]
async fn a_replaced_writer_cannot_change_the_index() {
    let f = fixture().await;
    let stale = f.repo.try_acquire("search", "a").await.unwrap().unwrap();
    f.repo.release(&stale).await.unwrap();
    f.repo.try_acquire("search", "b").await.unwrap().unwrap();
    let v2 = f
        .child(f.version, &[("src/late.rs", "fn too_late() {}\n")])
        .await;
    assert!(f.index.index_version(&v2, Some(&stale)).await.is_err());
    let reader = SearchIndex::open(f.repo.clone(), f.derive.clone(), config())
        .await
        .unwrap();
    let root2 = f.derive.get::<rill_core::RootTree>(&v2).await.unwrap();
    assert!(!reader.has_root(root2).await.unwrap());
}
