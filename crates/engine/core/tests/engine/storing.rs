//! Storing versions with what landing needs of them: a chain stored in
//! one transaction holds exactly the values derivation would compute, and
//! every tree node it built is in the store.

use std::sync::Arc;

use rill_core::derive::{Ancestry, ChangedPaths, RootTree};
use rill_core::{ChangeBuilder, DeriveManager, TreeOps};
use rill_types::ChangeVersion;

use crate::common::{content_at, fixture, t, Fixture};

/// A base and `n` versions each on the one before, built (not stored).
/// Contents are small, so they live inline and need no objects.
async fn chain(f: &Fixture, n: u64) -> (ChangeVersion, Vec<ChangeVersion>) {
    let base = ChangeBuilder::new(f.alice.clone())
        .message("base")
        .created_at(t(1))
        .file("dir/base.txt", "b\n")
        .unwrap()
        .build(f.repo.objects().as_ref())
        .await
        .unwrap();
    let mut parent = base.version_hash();
    let mut out = Vec::new();
    for i in 0..n {
        let cv = ChangeBuilder::new(f.alice.clone())
            .message(format!("v{i}"))
            .created_at(t(2 + i as i64))
            .parent(parent)
            .file(&format!("dir/sub{}/f{i}.txt", i % 3), format!("{i}\n"))
            .unwrap()
            .build(f.repo.objects().as_ref())
            .await
            .unwrap();
        parent = cv.version_hash();
        out.push(cv);
    }
    (base, out)
}

async fn store_base(f: &Fixture, base: &ChangeVersion) {
    let hash = f.repo.put_change_version(base).await.unwrap();
    f.derive.ensure_land_critical(&hash).await.unwrap();
}

/// A manager over `f`'s store with nothing cached, so reads hit storage.
fn fresh(f: &Fixture) -> Arc<DeriveManager> {
    DeriveManager::new(
        f.repo.clone(),
        TreeOps::new(f.repo.objects().clone(), 16),
        16,
    )
}

#[tokio::test]
async fn a_chain_stored_at_once_holds_what_derivation_computes() {
    let (chained, one_by_one) = (fixture().await, fixture().await);
    let (base, versions) = chain(&chained, 20).await;
    store_base(&chained, &base).await;
    store_base(&one_by_one, &base).await;
    let hashes = chained.derive.store_chain(&versions).await.unwrap();
    for cv in &versions {
        one_by_one.repo.put_change_version(cv).await.unwrap();
    }

    let stored = fresh(&chained);
    for hash in &hashes {
        assert!(chained.repo.has_change_version(hash).await.unwrap());
        assert_eq!(
            stored.get::<Ancestry>(hash).await.unwrap(),
            one_by_one.derive.get::<Ancestry>(hash).await.unwrap()
        );
        assert_eq!(
            stored.get::<RootTree>(hash).await.unwrap(),
            one_by_one.derive.get::<RootTree>(hash).await.unwrap()
        );
        assert_eq!(
            stored.get::<ChangedPaths>(hash).await.unwrap(),
            one_by_one.derive.get::<ChangedPaths>(hash).await.unwrap()
        );
    }
    let last = hashes.last().unwrap();
    let reread = Fixture {
        derive: stored,
        ..chained
    };
    for i in [0, 7, 19] {
        assert_eq!(
            content_at(&reread, last, &format!("dir/sub{}/f{i}.txt", i % 3))
                .await
                .as_deref(),
            Some(format!("{i}\n").as_bytes()),
            "every node the chain built is stored"
        );
    }
}

#[tokio::test]
async fn store_version_commits_a_version_with_its_values() {
    let f = fixture().await;
    let (base, versions) = chain(&f, 2).await;
    store_base(&f, &base).await;
    let first = f.derive.store_version(&versions[0]).await.unwrap();
    assert_eq!(
        f.derive.store_version(&versions[0]).await.unwrap(),
        first,
        "storing twice is harmless"
    );
    let stored = fresh(&f);
    assert_eq!(stored.get::<Ancestry>(&first).await.unwrap().depth, 1);
    let reread = Fixture {
        derive: stored,
        ..f
    };
    assert_eq!(
        content_at(&reread, &first, "dir/sub0/f0.txt")
            .await
            .as_deref(),
        Some(&b"0\n"[..])
    );
}
