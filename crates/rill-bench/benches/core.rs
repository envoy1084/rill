//! Hot paths that must stay O(size of change). `RILL_BENCH_FILES` sets the
//! repository size (default 100,000 files).

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rill_bench::{Corpus, Shape};
use rill_core::derive::RootTree;
use rill_core::diff::diff_lines;
use rill_core::{diff_trees, merge_file, rebase};
use rill_types::Timestamp;
use tokio::runtime::Runtime;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
}

fn benches(c: &mut Criterion) {
    let rt = runtime();
    let corpus = rt.block_on(Corpus::build(Shape::from_env(100_000)));
    let files = corpus.shape.files;
    let base_root = rt.block_on(corpus.derive.get::<RootTree>(&corpus.root)).unwrap();
    let next = rt.block_on(corpus.change(corpus.root, 10, 1));
    let next_root = rt.block_on(corpus.derive.get::<RootTree>(&next)).unwrap();
    let mut revision = 1000;

    c.bench_function(&format!("commit_10_files/{files}"), |b| {
        b.iter(|| {
            revision += 1;
            rt.block_on(async {
                let version = corpus.change(corpus.root, 10, revision).await;
                black_box(corpus.derive.get::<RootTree>(&version).await.unwrap())
            })
        })
    });

    c.bench_function(&format!("diff_adjacent_roots/{files}"), |b| {
        b.iter(|| {
            rt.block_on(diff_trees(corpus.derive.trees(), Some(base_root), Some(next_root)))
                .unwrap()
        })
    });

    let behind = rt.block_on(corpus.change(corpus.root, 10, 2));
    let mut head = corpus.root;
    for r in 3..23 {
        head = rt.block_on(corpus.change(head, 10, r));
    }
    c.bench_function(&format!("rebase_10_files_over_20/{files}"), |b| {
        b.iter_batched(
            || (),
            |_| rt.block_on(rebase(&corpus.derive, &behind, &head, Timestamp::from_millis(0))),
            BatchSize::SmallInput,
        )
    });

    let shape = Shape {
        lines: 2_000,
        ..corpus.shape
    };
    let (base, left, right) = (shape.content(1, 0), shape.content(1, 1), shape.content(1, 3));
    c.bench_function("diff_lines_2000", |b| {
        b.iter(|| diff_lines(black_box(&base), black_box(&left)))
    });
    c.bench_function("merge_file_2000", |b| {
        b.iter(|| merge_file(black_box(&base), black_box(&left), black_box(&right)))
    });
}

criterion_group! {
    name = core;
    config = Criterion::default().sample_size(20);
    targets = benches
}
criterion_main!(core);
