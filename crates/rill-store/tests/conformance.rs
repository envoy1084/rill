//! Backend conformance: one scenario, every backend. The memory backends are
//! the reference; any backend that diverges from them here is wrong.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_store::meta::{prefix_end, seq_key};
use rill_store::{
    Error, KeyRange, MemoryMetaStore, MemoryObjectStore, MetaStore, ObjectStore, Stores,
    Transaction,
};
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, ObjectHash, ObjectKind, Principal, Provenance,
    Timestamp, TrunkSeq,
};

/// Everything needed to run the scenario against one backend pair. The
/// tempdir (when any) is kept alive for the duration of the test.
struct Fixture {
    name: &'static str,
    stores: Stores,
    _keep: Option<tempfile::TempDir>,
}

async fn fixtures() -> Vec<Fixture> {
    let mut out = vec![Fixture {
        name: "memory",
        stores: Stores::new(
            Arc::new(MemoryObjectStore::new()),
            Arc::new(MemoryMetaStore::new()),
        ),
        _keep: None,
    }];
    #[cfg(feature = "local")]
    {
        let dir = tempfile::tempdir().unwrap();
        let stores = rill_store::LocalStores::open(dir.path()).await.unwrap();
        out.push(Fixture {
            name: "local",
            stores,
            _keep: Some(dir),
        });
    }
    #[cfg(feature = "s3")]
    {
        let objects = rill_store::ObjectStoreBackend::new(
            Arc::new(object_store::memory::InMemory::new()),
            "rill",
        );
        out.push(Fixture {
            name: "object_store",
            stores: Stores::new(Arc::new(objects), Arc::new(MemoryMetaStore::new())),
            _keep: None,
        });
    }
    out
}

fn b(s: &[u8]) -> Bytes {
    Bytes::copy_from_slice(s)
}

async fn objects_scenario(name: &str, store: &Arc<dyn ObjectStore>) {
    let small = b(b"hello");
    let large = Bytes::from((0..200_000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>());
    let empty = Bytes::new();
    for (kind, bytes) in [
        (ObjectKind::Blob, small.clone()),
        (ObjectKind::Chunk, large.clone()),
        (ObjectKind::Tree, empty.clone()),
    ] {
        let hash = ObjectHash::digest(kind, &bytes);
        assert!(!store.has(kind, &hash).await.unwrap(), "{name}: fresh");
        assert_eq!(store.get(kind, &hash).await.unwrap(), None, "{name}");
        store.put(kind, &hash, bytes.clone()).await.unwrap();
        store.put(kind, &hash, bytes.clone()).await.unwrap();
        assert!(store.has(kind, &hash).await.unwrap(), "{name}");
        assert_eq!(store.get(kind, &hash).await.unwrap(), Some(bytes), "{name}");
    }

    // Kinds are separate namespaces even for identical bytes.
    let blob_hash = ObjectHash::digest(ObjectKind::Blob, &small);
    assert!(!store.has(ObjectKind::Chunk, &blob_hash).await.unwrap());
    let as_chunk = ObjectHash::digest(ObjectKind::Chunk, &small);
    assert!(!store.has(ObjectKind::Chunk, &as_chunk).await.unwrap());

    let wrong = ObjectHash::digest(ObjectKind::Blob, b"not hello");
    let err = store
        .put(ObjectKind::Blob, &wrong, small.clone())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::HashMismatch { .. }), "{name}: {err}");
    assert!(!store.has(ObjectKind::Blob, &wrong).await.unwrap());

    let present = store
        .has_many(&[
            (ObjectKind::Blob, blob_hash),
            (ObjectKind::Blob, wrong),
            (
                ObjectKind::Chunk,
                ObjectHash::digest(ObjectKind::Chunk, &large),
            ),
        ])
        .await
        .unwrap();
    assert_eq!(present, vec![true, false, true], "{name}");
    let got = store
        .get_many(&[(ObjectKind::Blob, wrong), (ObjectKind::Blob, blob_hash)])
        .await
        .unwrap();
    assert_eq!(got, vec![None, Some(small)], "{name}");
}

async fn conditions_scenario(name: &str, meta: &Arc<dyn MetaStore>) {
    meta.transact(Transaction::new().set(b(b"c/a"), b(b"1")))
        .await
        .unwrap();

    let conflict = |tx: Transaction| async move {
        let err = meta.transact(tx).await.unwrap_err();
        assert!(err.is_conflict(), "{name}: expected conflict, got {err}");
    };
    conflict(
        Transaction::new()
            .expect(b(b"c/a"), Some(b(b"2")))
            .set(b(b"c/a"), b(b"x")),
    )
    .await;
    conflict(
        Transaction::new()
            .expect(b(b"c/a"), None)
            .set(b(b"c/a"), b(b"x")),
    )
    .await;
    conflict(
        Transaction::new()
            .expect_absent(b(b"c/a"))
            .set(b(b"c/z"), b(b"x")),
    )
    .await;
    conflict(
        Transaction::new()
            .expect_present(b(b"c/none"))
            .set(b(b"c/z"), b(b"x")),
    )
    .await;
    // A failed condition applies none of the mutations, even earlier ones.
    conflict(
        Transaction::new()
            .set(b(b"c/z"), b(b"x"))
            .expect_present(b(b"c/none")),
    )
    .await;
    assert_eq!(meta.get(b"c/a").await.unwrap(), Some(b(b"1")), "{name}");
    assert_eq!(meta.get(b"c/z").await.unwrap(), None, "{name}");

    meta.transact(
        Transaction::new()
            .expect(b(b"c/a"), Some(b(b"1")))
            .expect_absent(b(b"c/z"))
            .expect_present(b(b"c/a"))
            .set(b(b"c/a"), b(b"2"))
            .set(b(b"c/z"), b(b"z"))
            .delete(b(b"c/never"))
            .delete(b(b"c/z")),
    )
    .await
    .unwrap();
    assert_eq!(meta.get(b"c/a").await.unwrap(), Some(b(b"2")), "{name}");
    assert_eq!(meta.get(b"c/z").await.unwrap(), None, "{name}");

    let many = meta
        .get_many(&[b(b"c/a"), b(b"c/z"), b(b"c/a")])
        .await
        .unwrap();
    assert_eq!(many, vec![Some(b(b"2")), None, Some(b(b"2"))], "{name}");

    // Empty transactions are fine.
    meta.transact(Transaction::new()).await.unwrap();
}

async fn append_log_scenario(name: &str, meta: &Arc<dyn MetaStore>) {
    let p1 = b(b"log/one/");
    let p2 = b(b"log/two/");
    let out = meta
        .transact(
            Transaction::new()
                .append_log(p1.clone(), b(b"a"))
                .append_log(p1.clone(), b(b"b"))
                .append_log(p2.clone(), b(b"x")),
        )
        .await
        .unwrap();
    assert_eq!(out.appended, vec![1, 2, 1], "{name}");
    let out = meta
        .transact(Transaction::new().append_log(p1.clone(), b(b"c")))
        .await
        .unwrap();
    assert_eq!(out.appended, vec![3], "{name}");

    // Deleting the tail moves the sequence back: the log is numbered from
    // the highest existing key, matching the memory backend.
    meta.transact(Transaction::new().delete(seq_key(&p1, 3)))
        .await
        .unwrap();
    let out = meta
        .transact(Transaction::new().append_log(p1.clone(), b(b"c2")))
        .await
        .unwrap();
    assert_eq!(out.appended, vec![3], "{name}");

    let rows = meta.scan(KeyRange::prefix(p1.clone())).await.unwrap();
    assert_eq!(
        rows,
        vec![
            (seq_key(&p1, 1), b(b"a")),
            (seq_key(&p1, 2), b(b"b")),
            (seq_key(&p1, 3), b(b"c2")),
        ],
        "{name}"
    );
    assert_eq!(
        meta.get(&seq_key(&p2, 1)).await.unwrap(),
        Some(b(b"x")),
        "{name}"
    );

    // A failed condition means no sequence is consumed.
    let err = meta
        .transact(
            Transaction::new()
                .expect_present(b(b"log/nope"))
                .append_log(p1.clone(), b(b"d")),
        )
        .await
        .unwrap_err();
    assert!(err.is_conflict());
    let out = meta
        .transact(Transaction::new().append_log(p1.clone(), b(b"d")))
        .await
        .unwrap();
    assert_eq!(out.appended, vec![4], "{name}");
}

async fn scan_scenario(name: &str, meta: &Arc<dyn MetaStore>) {
    let mut tx = Transaction::new();
    for i in 0..10u8 {
        tx = tx.set(b(&[b's', b'/', b'0' + i]), b(&[i]));
    }
    // Neighbours outside the prefix on both sides.
    tx = tx.set(b(b"s."), b(b"lo")).set(b(b"s0"), b(b"hi"));
    meta.transact(tx).await.unwrap();

    let keys = |rows: Vec<(Bytes, Bytes)>| -> Vec<String> {
        rows.into_iter()
            .map(|(k, _)| String::from_utf8(k.to_vec()).unwrap())
            .collect()
    };

    let all = keys(meta.scan(KeyRange::prefix(b(b"s/"))).await.unwrap());
    assert_eq!(all.len(), 10, "{name}");
    assert_eq!(all[0], "s/0");
    assert_eq!(all[9], "s/9");

    let limited = keys(
        meta.scan(KeyRange::prefix(b(b"s/")).limit(3))
            .await
            .unwrap(),
    );
    assert_eq!(limited, vec!["s/0", "s/1", "s/2"], "{name}");

    let rev = keys(
        meta.scan(KeyRange::prefix(b(b"s/")).reverse().limit(2))
            .await
            .unwrap(),
    );
    assert_eq!(rev, vec!["s/9", "s/8"], "{name}");

    let zero = meta
        .scan(KeyRange::prefix(b(b"s/")).limit(0))
        .await
        .unwrap();
    assert!(zero.is_empty(), "{name}");

    let between = keys(
        meta.scan(KeyRange::between(b(b"s/3"), b(b"s/6")))
            .await
            .unwrap(),
    );
    assert_eq!(between, vec!["s/3", "s/4", "s/5"], "{name}");

    let between_rev = keys(
        meta.scan(KeyRange::between(b(b"s/3"), b(b"s/6")).reverse())
            .await
            .unwrap(),
    );
    assert_eq!(between_rev, vec!["s/5", "s/4", "s/3"], "{name}");

    let none = meta.scan(KeyRange::prefix(b(b"s/zzz"))).await.unwrap();
    assert!(none.is_empty(), "{name}");

    // Explicit bounds, including an inclusive end and an excluded start.
    let range = KeyRange {
        start: std::ops::Bound::Excluded(b(b"s/3")),
        end: std::ops::Bound::Included(b(b"s/6")),
        limit: usize::MAX,
        reverse: false,
    };
    assert_eq!(
        keys(meta.scan(range).await.unwrap()),
        vec!["s/4", "s/5", "s/6"],
        "{name}"
    );

    // Unbounded on both sides sees everything written so far.
    let everything = KeyRange {
        start: std::ops::Bound::Unbounded,
        end: std::ops::Bound::Unbounded,
        limit: usize::MAX,
        reverse: false,
    };
    let all_rows = meta.scan(everything).await.unwrap();
    assert!(all_rows.len() >= 12, "{name}");
    assert!(
        all_rows.windows(2).all(|w| w[0].0 < w[1].0),
        "{name}: sorted"
    );
}

async fn prefix_end_scenario(name: &str, meta: &Arc<dyn MetaStore>) {
    // A prefix ending in 0xFF rolls over into the previous byte, and an
    // all-0xFF prefix has no end at all; scans must honour both.
    let p = b(&[b'p', 0xFF]);
    let inside = b(&[b'p', 0xFF, 0x00]);
    let inside2 = b(&[b'p', 0xFF, 0xFF]);
    let after = b(b"q");
    let before = b(&[b'p', 0xFE, 0xFF]);
    meta.transact(
        Transaction::new()
            .set(inside.clone(), b(b"1"))
            .set(inside2.clone(), b(b"2"))
            .set(after.clone(), b(b"3"))
            .set(before.clone(), b(b"4")),
    )
    .await
    .unwrap();
    assert_eq!(prefix_end(&p).unwrap(), after);
    let rows = meta.scan(KeyRange::prefix(p.clone())).await.unwrap();
    assert_eq!(
        rows,
        vec![(inside.clone(), b(b"1")), (inside2.clone(), b(b"2"))],
        "{name}"
    );

    let all_ff = b(&[0xFF, 0xFF]);
    let inside_ff = b(&[0xFF, 0xFF, 0x01]);
    meta.transact(Transaction::new().set(inside_ff.clone(), b(b"5")))
        .await
        .unwrap();
    assert!(prefix_end(&all_ff).is_none());
    let rows = meta.scan(KeyRange::prefix(all_ff.clone())).await.unwrap();
    assert_eq!(rows, vec![(inside_ff.clone(), b(b"5"))], "{name}");
    let rows = meta
        .scan(KeyRange::prefix(all_ff).reverse().limit(1))
        .await
        .unwrap();
    assert_eq!(rows, vec![(inside_ff, b(b"5"))], "{name}");
}

fn cv(msg: &str, parents: Vec<ChangeVersionHash>) -> ChangeVersion {
    ChangeVersion {
        change_id: ChangeId::new(),
        parents,
        predecessor: None,
        subtree_changes: BTreeMap::new(),
        file_changes: BTreeMap::new(),
        author: Principal::human("a"),
        provenance: Provenance::default(),
        intent: None,
        message: msg.into(),
        created_at: Timestamp::from_millis(1),
        extra: BTreeMap::new(),
    }
}

async fn repo_scenario(name: &str, stores: &Stores) {
    let config = stores.create_repo("conf", "desc").await.unwrap();
    assert!(
        stores
            .create_repo("conf", "")
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}"
    );
    assert_eq!(
        stores.get_repo_by_name("conf").await.unwrap(),
        Some(config.clone())
    );
    let repo = stores.repo(config.id);

    let a_cv = cv("a", vec![]);
    let a = repo.put_change_version(&a_cv).await.unwrap();
    assert_eq!(
        repo.put_change_version(&a_cv).await.unwrap(),
        a,
        "{name}: idempotent"
    );
    let b_hash = repo.put_change_version(&cv("b", vec![a])).await.unwrap();
    assert_eq!(
        repo.get_change_version(&a).await.unwrap(),
        Some(a_cv.clone())
    );
    assert_eq!(
        repo.change_versions(&a_cv.change_id).await.unwrap(),
        vec![a]
    );
    assert_eq!(repo.log_head().await.unwrap(), 2, "{name}");
    assert_eq!(
        repo.log_after(0, 10).await.unwrap(),
        vec![(1, a), (2, b_hash)],
        "{name}"
    );

    assert_eq!(repo.trunk_head().await.unwrap(), None);
    assert_eq!(repo.land_on_trunk(None, a).await.unwrap(), TrunkSeq(1));
    assert!(
        repo.land_on_trunk(None, b_hash)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: stale expectation"
    );
    assert_eq!(
        repo.land_on_trunk(Some(a), b_hash).await.unwrap(),
        TrunkSeq(2)
    );
    assert_eq!(
        repo.trunk_head().await.unwrap(),
        Some((TrunkSeq(2), b_hash))
    );
    assert_eq!(repo.trunk_at(TrunkSeq(1)).await.unwrap(), Some(a));
    assert_eq!(repo.trunk_seq_of(&b_hash).await.unwrap(), Some(TrunkSeq(2)));
    assert_eq!(
        repo.trunk_range(TrunkSeq(0), TrunkSeq(2)).await.unwrap(),
        vec![(TrunkSeq(1), a), (TrunkSeq(2), b_hash)]
    );
    let info = repo.get_change_info(&b_hash).await.unwrap().unwrap();
    assert_eq!(info.trunk_seq, Some(TrunkSeq(2)), "{name}");

    // Ref CAS: create, wrong expectation, update, delete.
    let feature = rill_types::RefName::new("feature").unwrap();
    repo.update_ref(&feature, None, Some(a)).await.unwrap();
    assert!(repo
        .update_ref(&feature, None, Some(b_hash))
        .await
        .unwrap_err()
        .is_conflict());
    repo.update_ref(&feature, Some(a), Some(b_hash))
        .await
        .unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), Some(b_hash));
    let refs = repo.list_refs().await.unwrap();
    assert_eq!(refs.len(), 2, "{name}: trunk + feature");
    repo.update_ref(&feature, Some(b_hash), None).await.unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), None);

    repo.add_relation("doc", "owner", "alice").await.unwrap();
    repo.add_relation("doc", "viewer", "bob").await.unwrap();
    assert_eq!(
        repo.relations_of("doc").await.unwrap(),
        vec![
            ("owner".to_string(), "alice".to_string()),
            ("viewer".to_string(), "bob".to_string())
        ]
    );
    repo.remove_relation("doc", "owner", "alice").await.unwrap();
    assert_eq!(repo.relations_of("doc").await.unwrap().len(), 1);
}

#[tokio::test]
async fn object_stores_conform() {
    for f in fixtures().await {
        objects_scenario(f.name, &f.stores.objects).await;
    }
}

#[tokio::test]
async fn meta_stores_conform() {
    for f in fixtures().await {
        conditions_scenario(f.name, &f.stores.meta).await;
        append_log_scenario(f.name, &f.stores.meta).await;
        scan_scenario(f.name, &f.stores.meta).await;
        prefix_end_scenario(f.name, &f.stores.meta).await;
    }
}

#[tokio::test]
async fn repo_stores_conform() {
    for f in fixtures().await {
        repo_scenario(f.name, &f.stores).await;
    }
}

#[cfg(feature = "local")]
#[tokio::test]
async fn redb_temporary_matches_memory() {
    let store: Arc<dyn MetaStore> = Arc::new(rill_store::RedbMetaStore::temporary().unwrap());
    conditions_scenario("redb-temp", &store).await;
    append_log_scenario("redb-temp", &store).await;
    scan_scenario("redb-temp", &store).await;
    prefix_end_scenario("redb-temp", &store).await;
}
