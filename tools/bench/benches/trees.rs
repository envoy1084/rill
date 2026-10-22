//! Trees at scale: a one-file commit, a full walk and a tree diff in a
//! deep repository (`RILL_BENCH_FILES`, default 100,000 files), and edits,
//! lookups and diffs in one 200,000-entry directory. Each one-file commit
//! also reports how many tree nodes it wrote.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rill_bench::{Corpus, Shape};
use rill_core::derive::RootTree;
use rill_core::diff_trees;
use rill_types::{RepoPathBuf, SparseProfile};
use tokio::runtime::Runtime;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
}

fn nodes_per_commit(rt: &Runtime, corpus: &Corpus, revision: usize) -> u64 {
    let trees = corpus.derive.trees();
    let before = trees.nodes_written();
    rt.block_on(async {
        let version = corpus.change(corpus.root, 1, revision).await;
        corpus.derive.get::<RootTree>(&version).await.unwrap();
    });
    trees.nodes_written() - before
}

fn deep(c: &mut Criterion) {
    let rt = runtime();
    let corpus = rt.block_on(Corpus::build(Shape::from_env(100_000)));
    let files = corpus.shape.files;
    let trees = corpus.derive.trees().clone();
    let root = rt
        .block_on(corpus.derive.get::<RootTree>(&corpus.root))
        .unwrap();
    eprintln!(
        "one-file commit in {files} files wrote {} tree nodes",
        nodes_per_commit(&rt, &corpus, 5_000)
    );

    let mut revision = 1_000;
    c.bench_function(&format!("commit_1_file/{files}"), |b| {
        b.iter(|| {
            revision += 1;
            rt.block_on(async {
                let version = corpus.change(corpus.root, 1, revision).await;
                black_box(corpus.derive.get::<RootTree>(&version).await.unwrap())
            })
        })
    });
    c.bench_function(&format!("walk_all/{files}"), |b| {
        b.iter(|| {
            rt.block_on(trees.walk(root, &RepoPathBuf::root(), &SparseProfile::full()))
                .unwrap()
                .len()
        })
    });
    let next = rt.block_on(corpus.change(corpus.root, 1, 2));
    let next_root = rt.block_on(corpus.derive.get::<RootTree>(&next)).unwrap();
    c.bench_function(&format!("diff_1_file/{files}"), |b| {
        b.iter(|| {
            rt.block_on(diff_trees(&trees, Some(root), Some(next_root)))
                .unwrap()
        })
    });
}

fn wide(c: &mut Criterion) {
    let rt = runtime();
    let entries = 200_000;
    let corpus = rt.block_on(Corpus::build(Shape {
        fanout: entries,
        ..Shape::with_files(entries)
    }));
    let trees = corpus.derive.trees().clone();
    let root = rt
        .block_on(corpus.derive.get::<RootTree>(&corpus.root))
        .unwrap();
    eprintln!(
        "one-file commit in a {entries}-entry directory wrote {} tree nodes",
        nodes_per_commit(&rt, &corpus, 1)
    );

    let mut revision = 1_000;
    c.bench_function(&format!("wide_commit_1_file/{entries}"), |b| {
        b.iter(|| {
            revision += 1;
            rt.block_on(async {
                let version = corpus.change(corpus.root, 1, revision).await;
                black_box(corpus.derive.get::<RootTree>(&version).await.unwrap())
            })
        })
    });
    let name = corpus.shape.path(123_456);
    c.bench_function(&format!("wide_lookup/{entries}"), |b| {
        b.iter(|| rt.block_on(trees.entry(&root, &name)).unwrap())
    });
    let next = rt.block_on(corpus.change(corpus.root, 1, 2));
    let next_root = rt.block_on(corpus.derive.get::<RootTree>(&next)).unwrap();
    c.bench_function(&format!("wide_diff_1_file/{entries}"), |b| {
        b.iter(|| {
            rt.block_on(diff_trees(&trees, Some(root), Some(next_root)))
                .unwrap()
        })
    });
}

criterion_group! {
    name = trees;
    config = Criterion::default().sample_size(20);
    targets = deep, wide
}
criterion_main!(trees);
