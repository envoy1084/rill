//! Metadata store round trips on the in-process backends: a conditional
//! write, a point read and a 100-row scan.

use std::hint::black_box;
use std::sync::Arc;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion};
use rill_store::{KeyRange, MemoryMetaStore, MetaStore, RedbMetaStore, Transaction};
use tokio::runtime::Runtime;

fn stores() -> Vec<(&'static str, Arc<dyn MetaStore>)> {
    vec![
        ("memory", Arc::new(MemoryMetaStore::new())),
        ("redb", Arc::new(RedbMetaStore::temporary().expect("redb"))),
    ]
}

fn benches(c: &mut Criterion) {
    let rt = Runtime::new().expect("runtime");
    for (name, meta) in stores() {
        rt.block_on(async {
            let mut tx = Transaction::new();
            for i in 0..1000u32 {
                tx = tx.set(
                    Bytes::from(format!("row/{i:04}")),
                    Bytes::from(vec![7u8; 200]),
                );
            }
            meta.transact(tx).await.unwrap();
        });
        let mut n = 0u64;
        c.bench_function(&format!("meta_conditional_write/{name}"), |b| {
            b.iter(|| {
                n += 1;
                rt.block_on(async {
                    let key = Bytes::from(format!("cas/{}", n % 64));
                    let current = meta.get(&key).await.unwrap();
                    let tx = Transaction::new()
                        .expect(key.clone(), current)
                        .set(key, Bytes::copy_from_slice(&n.to_be_bytes()));
                    black_box(meta.transact(tx).await.unwrap())
                })
            })
        });
        c.bench_function(&format!("meta_get/{name}"), |b| {
            b.iter(|| rt.block_on(async { black_box(meta.get(b"row/0500").await.unwrap()) }))
        });
        c.bench_function(&format!("meta_scan_100/{name}"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let range = KeyRange::prefix(Bytes::from_static(b"row/")).limit(100);
                    black_box(meta.scan(range).await.unwrap())
                })
            })
        });
    }
}

criterion_group!(meta, benches);
criterion_main!(meta);
