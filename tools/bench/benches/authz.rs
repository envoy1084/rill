//! Relation checks over a repository with many tuples: the indexed
//! snapshot against the full scan it replaced, and building a read grant.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rill_policy::grants::{GrantEntry, GrantSigner};
use rill_policy::rebac::{dir_object, inheritance_chain, Snapshot, Tuple};
use rill_policy::{Relation, Subject};
use rill_types::{ObjectHash, ObjectKind, Principal, RepoId, RepoPathBuf, Timestamp};

/// 10,000 directories, each with a team writer and a user reader.
fn snapshot() -> Snapshot {
    let mut tuples = Vec::new();
    for i in 0..5_000 {
        let dir = RepoPathBuf::new(format!("svc{}/mod{}", i % 100, i)).unwrap();
        tuples.push(Tuple {
            object: dir_object(&dir),
            relation: Relation::Writer,
            subject: Subject::Team(format!("team{}", i % 50)),
        });
        tuples.push(Tuple {
            object: dir_object(&dir),
            relation: Relation::Reader,
            subject: Subject::principal(&Principal::human(format!("user{i}"))),
        });
    }
    Snapshot::from_tuples(1, tuples)
}

fn benches(c: &mut Criterion) {
    let snap = snapshot();
    let who = Principal::human("user4321");
    let object = dir_object(&RepoPathBuf::new("svc21/mod4321/src/lib.rs").unwrap());
    c.bench_function("authz_check/indexed_10k_tuples", |b| {
        b.iter(|| black_box(snap.check(&who, &[], Relation::Reader, &object)))
    });
    c.bench_function("authz_check/full_scan_10k_tuples", |b| {
        b.iter(|| {
            let accepted = Relation::Reader.satisfied_by();
            let chain = inheritance_chain(&object);
            black_box(snap.tuples().iter().any(|t| {
                accepted.contains(&t.relation)
                    && chain.contains(&t.object)
                    && t.subject.matches(&who, &[])
            }))
        })
    });
    let signer = GrantSigner::derive(1, &[7u8; 32]).unwrap();
    let entries: Vec<GrantEntry> = (0..100u32)
        .map(|i| GrantEntry {
            kind: ObjectKind::Blob,
            hash: ObjectHash::digest(ObjectKind::Blob, &i.to_be_bytes()),
            path: RepoPathBuf::new(format!("src/file{i}.rs")).unwrap(),
        })
        .collect();
    let repo = RepoId::new();
    c.bench_function("read_grant/sign_100_entries", |b| {
        b.iter(|| {
            black_box(
                signer
                    .sign(&who, repo, entries.clone(), Timestamp::now(), 600_000)
                    .unwrap(),
            )
        })
    });
}

criterion_group!(authz, benches);
criterion_main!(authz);
