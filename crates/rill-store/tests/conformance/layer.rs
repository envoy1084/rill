//! What the keyspace layer adds: stamps, counters, split values and the
//! transaction budget.

use std::sync::Arc;

use bytes::Bytes;
use rill_store::keyspace::{MAX_KEY_BYTES, ROW_BYTES, TX_BYTES};
use rill_store::meta::{stamp_from_key, stamp_key};
use rill_store::{Error, KeyRange, MetaStore, Transaction};

use crate::b;

pub async fn stamps(name: &str, meta: &Arc<dyn MetaStore>) {
    let log = b(b"stamped/");
    let first = meta
        .transact(
            Transaction::new()
                .append_stamped(log.clone(), b(b"a"))
                .append_stamped(log.clone(), b(b"b")),
        )
        .await
        .unwrap()
        .stamped;
    let second = meta
        .transact(Transaction::new().append_stamped(log.clone(), b(b"c")))
        .await
        .unwrap()
        .stamped;
    assert_eq!(first.len(), 2, "{name}");
    assert!(first[0] < first[1] && first[1] < second[0], "{name}");
    assert_eq!(
        first[0].as_bytes()[..10],
        first[1].as_bytes()[..10],
        "{name}"
    );

    let rows = meta.scan(KeyRange::prefix(log.clone())).await.unwrap();
    let got: Vec<_> = rows
        .iter()
        .map(|(k, v)| (stamp_from_key(log.len(), k).unwrap(), v.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (first[0], b(b"a")),
            (first[1], b(b"b")),
            (second[0], b(b"c"))
        ],
        "{name}"
    );
    assert_eq!(
        meta.get(&stamp_key(&log, &first[1])).await.unwrap(),
        Some(b(b"b")),
        "{name}"
    );

    let err = meta
        .transact(
            Transaction::new()
                .expect_present(b(b"stamped-missing"))
                .append_stamped(log.clone(), b(b"d")),
        )
        .await
        .unwrap_err();
    assert!(err.is_conflict(), "{name}");
    assert_eq!(meta.scan(KeyRange::prefix(log)).await.unwrap().len(), 3);
}

pub async fn counters(name: &str, meta: &Arc<dyn MetaStore>) {
    let key = b(b"count/a");
    assert_eq!(meta.counter(&key).await.unwrap(), 0, "{name}");
    meta.transact(Transaction::new().add(key.clone(), 5).add(key.clone(), -2))
        .await
        .unwrap();
    assert_eq!(meta.counter(&key).await.unwrap(), 3, "{name}");
    meta.transact(Transaction::new().add(key.clone(), i64::MIN))
        .await
        .unwrap();
    assert_eq!(
        meta.counter(&key).await.unwrap(),
        3i64.wrapping_add(i64::MIN)
    );
    // Counters are kept apart from values under the same key.
    assert_eq!(meta.get(&key).await.unwrap(), None, "{name}");
}

fn pattern(len: usize, seed: u8) -> Bytes {
    Bytes::from(
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect::<Vec<_>>(),
    )
}

pub async fn split_values(name: &str, meta: &Arc<dyn MetaStore>) {
    let sizes = [
        0,
        1,
        ROW_BYTES - 2,
        ROW_BYTES - 1,
        ROW_BYTES,
        ROW_BYTES + 1,
        3 * ROW_BYTES,
        TX_BYTES / 2,
    ];
    for (i, len) in sizes.into_iter().enumerate() {
        let key = b(format!("split/{i:02}").as_bytes());
        let value = pattern(len, i as u8);
        meta.set(key.clone(), value.clone()).await.unwrap();
        assert_eq!(
            meta.get(&key).await.unwrap(),
            Some(value),
            "{name}: {len} bytes"
        );
    }
    let rows = meta.scan(KeyRange::prefix(b(b"split/"))).await.unwrap();
    assert_eq!(rows.len(), sizes.len(), "{name}");
    for ((_, value), len) in rows.iter().zip(sizes) {
        assert_eq!(value.len(), len, "{name}");
    }

    // Large to small, small to large, and conditions on large values.
    let key = b(b"split/swap");
    let large = pattern(2 * ROW_BYTES + 7, 1);
    let other = pattern(2 * ROW_BYTES + 7, 2);
    meta.set(key.clone(), large.clone()).await.unwrap();
    meta.set(key.clone(), b(b"small")).await.unwrap();
    assert_eq!(meta.get(&key).await.unwrap(), Some(b(b"small")), "{name}");
    meta.set(key.clone(), large.clone()).await.unwrap();
    let stale = Transaction::new()
        .expect(key.clone(), Some(other.clone()))
        .set(key.clone(), b(b"x"));
    assert!(meta.transact(stale).await.unwrap_err().is_conflict());
    meta.transact(
        Transaction::new()
            .expect(key.clone(), Some(large))
            .set(key.clone(), other.clone()),
    )
    .await
    .unwrap();
    assert_eq!(meta.get(&key).await.unwrap(), Some(other), "{name}");
    meta.transact(Transaction::new().delete(key.clone()))
        .await
        .unwrap();
    assert_eq!(meta.get(&key).await.unwrap(), None, "{name}");

    // Large values in logs.
    let log = b(b"split-log/");
    let entry = pattern(ROW_BYTES * 2, 9);
    let out = meta
        .transact(Transaction::new().append_log(log.clone(), entry.clone()))
        .await
        .unwrap();
    assert_eq!(out.appended, vec![1], "{name}");
    let rows = meta.scan(KeyRange::prefix(log)).await.unwrap();
    assert_eq!(rows.len(), 1, "{name}");
    assert_eq!(rows[0].1, entry, "{name}");
}

pub async fn budget(name: &str, meta: &Arc<dyn MetaStore>) {
    let too_large = |r: rill_store::Result<rill_store::TxOutcome>| {
        assert!(
            matches!(r, Err(Error::TooLarge(_))),
            "{name}: expected too large, got {r:?}"
        );
    };
    too_large(
        meta.transact(Transaction::new().set(Bytes::from(vec![b'k'; MAX_KEY_BYTES + 1]), b(b"v")))
            .await,
    );
    too_large(
        meta.transact(
            Transaction::new()
                .set(b(b"budget/a"), pattern(TX_BYTES / 2 + 1, 0))
                .set(b(b"budget/b"), pattern(TX_BYTES / 2 + 1, 1)),
        )
        .await,
    );
    too_large(
        meta.transact(Transaction::new().append_stamped(b(b"budget/s"), pattern(ROW_BYTES, 0)))
            .await,
    );
    assert_eq!(meta.get(b"budget/a").await.unwrap(), None, "{name}");
    meta.transact(Transaction::new().set(Bytes::from(vec![b'k'; MAX_KEY_BYTES]), b(b"v")))
        .await
        .unwrap();
}
