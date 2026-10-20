//! The basic metadata store contract: conditions, logs and scans.

use std::sync::Arc;

use bytes::Bytes;
use rill_store::meta::{prefix_end, seq_key};
use rill_store::{KeyRange, MetaStore, Transaction};

use crate::b;

pub async fn conditions(name: &str, meta: &Arc<dyn MetaStore>) {
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

pub async fn append_log(name: &str, meta: &Arc<dyn MetaStore>) {
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

pub async fn scan(name: &str, meta: &Arc<dyn MetaStore>) {
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

pub async fn prefix_bounds(name: &str, meta: &Arc<dyn MetaStore>) {
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
