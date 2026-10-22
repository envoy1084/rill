//! Concurrent writers: the properties the lander and every log rely on.

use std::collections::BTreeSet;
use std::sync::Arc;

use bytes::Bytes;
use futures::future::join_all;
use rill_store::{KeyRange, MetaStore, Stores, Transaction};
use rill_types::TrunkSeq;

use crate::{b, repo::cv};

const TASKS: usize = 32;

/// Every task increments one key by compare-and-swap. Each old value is
/// won by exactly one task, so no update is lost or applied twice.
pub async fn compare_and_swap(name: &str, meta: &Arc<dyn MetaStore>) {
    const ROUNDS: u64 = 8;
    let key = b(b"race/cas");
    let tasks = (0..TASKS).map(|_| {
        let meta = Arc::clone(meta);
        let key = key.clone();
        tokio::spawn(async move {
            let mut won = Vec::new();
            while (won.len() as u64) < ROUNDS {
                let current = meta.get(&key).await.unwrap();
                let n = current
                    .as_deref()
                    .map_or(0, |v| u64::from_be_bytes(v.try_into().unwrap()));
                let tx = Transaction::new()
                    .expect(key.clone(), current)
                    .set(key.clone(), Bytes::copy_from_slice(&(n + 1).to_be_bytes()));
                match meta.transact(tx).await {
                    Ok(_) => won.push(n),
                    Err(e) if e.is_conflict() => {}
                    Err(e) => panic!("{e}"),
                }
            }
            won
        })
    });
    let mut won: Vec<u64> = join_all(tasks)
        .await
        .into_iter()
        .flat_map(|r| r.unwrap())
        .collect();
    won.sort_unstable();
    let total = TASKS as u64 * ROUNDS;
    assert_eq!(won, (0..total).collect::<Vec<_>>(), "{name}");
    let last = meta.get(&key).await.unwrap().unwrap();
    assert_eq!(
        u64::from_be_bytes(last[..].try_into().unwrap()),
        total,
        "{name}"
    );
}

/// Landers racing for trunk each land once, at gap-free sequences.
pub async fn landing(name: &str, stores: &Stores) {
    let repo = stores.repo(stores.create_repo("race", "").await.unwrap().id);
    let mut versions = Vec::new();
    for i in 0..TASKS {
        versions.push(
            repo.put_change_version(&cv(&format!("v{i}"), vec![]))
                .await
                .unwrap(),
        );
    }
    let tasks = versions.iter().map(|&mine| {
        let repo = repo.clone();
        tokio::spawn(async move {
            loop {
                let head = repo.trunk_head().await.unwrap().map(|(_, h)| h);
                match repo.land_on_trunk(head, mine).await {
                    Ok(seq) => return seq,
                    Err(e) if e.is_conflict() => {}
                    Err(e) => panic!("{e}"),
                }
            }
        })
    });
    let mut seqs: Vec<TrunkSeq> = join_all(tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    seqs.sort_unstable();
    assert_eq!(
        seqs,
        (1..=TASKS as u64).map(TrunkSeq).collect::<Vec<_>>(),
        "{name}"
    );
    let trunk = repo
        .trunk_range(TrunkSeq::ZERO, TrunkSeq(TASKS as u64))
        .await
        .unwrap();
    let landed: BTreeSet<_> = trunk.iter().map(|(_, h)| *h).collect();
    assert_eq!(landed, versions.iter().copied().collect(), "{name}");
}

/// Concurrent adds and stamped appends never conflict and never lose one.
pub async fn blind_writes(name: &str, meta: &Arc<dyn MetaStore>) {
    const EACH: usize = 10;
    let counter = b(b"race/counter");
    let log = b(b"race/log/");
    let tasks = (0..TASKS).map(|t| {
        let meta = Arc::clone(meta);
        let (counter, log) = (counter.clone(), log.clone());
        tokio::spawn(async move {
            let mut stamps = Vec::new();
            for i in 0..EACH {
                let out = meta
                    .transact(
                        Transaction::new()
                            .add(counter.clone(), 1)
                            .append_stamped(log.clone(), Bytes::from(format!("{t}/{i}"))),
                    )
                    .await
                    .unwrap();
                stamps.extend(out.stamped);
            }
            stamps
        })
    });
    let stamps: Vec<_> = join_all(tasks)
        .await
        .into_iter()
        .flat_map(|r| r.unwrap())
        .collect();
    let total = TASKS * EACH;
    assert_eq!(
        meta.counter(&counter).await.unwrap(),
        total as i64,
        "{name}"
    );
    let unique: BTreeSet<_> = stamps.iter().copied().collect();
    assert_eq!(unique.len(), total, "{name}: stamps are unique");
    let rows = meta.scan(KeyRange::prefix(log.clone())).await.unwrap();
    let scanned: Vec<_> = rows
        .iter()
        .map(|(k, _)| rill_store::meta::stamp_from_key(log.len(), k).unwrap())
        .collect();
    assert_eq!(
        scanned,
        unique.into_iter().collect::<Vec<_>>(),
        "{name}: log order is stamp order"
    );
}

/// Storing one version from many tasks at once logs it once.
pub async fn duplicate_puts(name: &str, stores: &Stores) {
    let repo = stores.repo(stores.create_repo("dupes", "").await.unwrap().id);
    let version = cv("same", vec![]);
    let tasks = (0..TASKS).map(|_| {
        let (repo, version) = (repo.clone(), version.clone());
        tokio::spawn(async move { repo.put_change_version(&version).await.unwrap() })
    });
    let hashes: BTreeSet<_> = join_all(tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(hashes.len(), 1, "{name}");
    assert_eq!(repo.log_after(None, 100).await.unwrap().len(), 1, "{name}");
    assert_eq!(
        repo.change_versions(&version.change_id)
            .await
            .unwrap()
            .len(),
        1,
        "{name}"
    );
}
