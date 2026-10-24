//! `search_gate`: the phase 26 exit gate for code search.
//!
//! Builds (or reuses) a synthetic repository of `--files` source files (10M
//! by default) on local stores, with content inline in the trees, and
//! indexes it with a full walk. Then, for `--duration` seconds, it runs the
//! query mix open-loop at `--qps` against a reader index whose segments
//! are memory-mapped from its own cache directory, while a writer lands a
//! change of 1-20 files every `--land-every-ms` and indexes it. It reports
//! query latency by kind and achieved throughput, and index lag: from a
//! landing's root being written to the reader seeing it indexed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use hdrhistogram::Histogram;
use parking_lot::{Mutex, RwLock};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rill_core::tree::Edit;
use rill_core::{DeriveManager, TreeBuilder, TreeOps};
use rill_load::search_gate::{corpus, queries};
use rill_search::{IndexConfig, PathView, Scope, SearchIndex};
use rill_store::local::LocalStores;
use rill_store::RepoStore;
use rill_types::{ContentRef, FileKind, RepoPath, RepoPathBuf, TreeHash};
use serde_json::json;

#[derive(Parser)]
#[command(about = "Phase 26 exit gate: search throughput and latency at 10M files, index lag")]
struct Args {
    /// Files in the synthetic repository.
    #[arg(long, default_value_t = 10_000_000)]
    files: usize,
    /// Queries per second offered.
    #[arg(long, default_value_t = 1_500)]
    qps: u64,
    /// Seconds of load.
    #[arg(long, default_value_t = 60)]
    duration: u64,
    /// Milliseconds between landings during the load.
    #[arg(long, default_value_t = 2_000)]
    land_every_ms: u64,
    /// Where the stores and segment caches live; reused when present.
    #[arg(long, default_value = "/tmp/rill-search-gate")]
    dir: PathBuf,
    /// Queries in flight before new ones count as dropped.
    #[arg(long, default_value_t = 4_096)]
    max_in_flight: u64,
}

const TARGET_P99: Duration = Duration::from_secs(1);
const TARGET_LAG_P99: Duration = Duration::from_secs(60);
/// Top-level directories built per tree batch.
const TOPS_PER_BATCH: usize = 10;

fn rel(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).expect("generated paths are valid")
}

fn inline(text: String) -> Edit {
    Edit::Set(ContentRef::inline(text.into_bytes()), FileKind::Regular)
}

/// The synthetic tree, built in batches of top-level directories.
async fn build_tree(trees: &TreeOps, files: usize) -> anyhow::Result<TreeHash> {
    let zipf = Arc::new(corpus::Zipf::default());
    let per_top = files.div_ceil(corpus::TOP_DIRS).max(1);
    let mut root = None;
    let started = Instant::now();
    for first_top in (0..corpus::TOP_DIRS).step_by(TOPS_PER_BATCH) {
        let lo = (first_top * per_top).min(files);
        let hi = ((first_top + TOPS_PER_BATCH) * per_top).min(files);
        if lo >= hi {
            break;
        }
        let zipf = zipf.clone();
        let edits = tokio::task::spawn_blocking(move || {
            let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
            let chunk = (hi - lo).div_ceil(threads).max(1);
            std::thread::scope(|s| {
                let parts: Vec<_> = (lo..hi)
                    .step_by(chunk)
                    .map(|start| {
                        let zipf = &zipf;
                        s.spawn(move || {
                            (start..(start + chunk).min(hi))
                                .map(|i| (corpus::path(i, files), corpus::content(i, 0, zipf)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                parts
                    .into_iter()
                    .flat_map(|p| p.join().expect("generator threads do not panic"))
                    .collect::<Vec<_>>()
            })
        })
        .await?;
        let mut builder = TreeBuilder::new();
        for (path, text) in edits {
            builder.edit(&rel(&path), inline(text));
        }
        root = Some(builder.build(trees, root).await?);
        eprintln!(
            "  tree: {hi} of {files} files ({:.0}s)",
            started.elapsed().as_secs_f64()
        );
    }
    root.context("no files")
}

/// The repository and its synthetic root, built on first use.
async fn repository(args: &Args) -> anyhow::Result<(RepoStore, TreeOps, TreeHash, bool)> {
    let stores = LocalStores::open(args.dir.join("store")).await?;
    let config = match stores.get_repo_by_name("gate").await? {
        Some(c) => c,
        None => stores.create_repo("gate", "search gate").await?,
    };
    let repo = stores.repo(config.id);
    let trees = TreeOps::new(repo.objects().clone(), 4_096);
    let key = format!("search-gate-root-{}", args.files);
    if let Some(hex) = repo.get_misc(&key).await? {
        let root: TreeHash = std::str::from_utf8(&hex)?.parse()?;
        return Ok((repo, trees, root, true));
    }
    eprintln!("building a {}-file tree", args.files);
    let root = build_tree(&trees, args.files).await?;
    repo.set_misc(&key, root.to_string().into()).await?;
    Ok((repo, trees, root, false))
}

fn index_config(dir: PathBuf) -> IndexConfig {
    IndexConfig {
        cache_dir: Some(dir),
        ..IndexConfig::default()
    }
}

fn histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(1, 600_000_000, 3).expect("the bounds are valid")
}

fn summary(h: &Histogram<u64>) -> serde_json::Value {
    let ms = |q: f64| h.value_at_quantile(q) as f64 / 1_000.0;
    json!({
        "count": h.len(),
        "p50_ms": ms(0.5),
        "p90_ms": ms(0.9),
        "p99_ms": ms(0.99),
        "max_ms": h.max() as f64 / 1_000.0,
    })
}

/// The scope queries run against: the newest root the reader has seen.
type Current = Arc<RwLock<Scope>>;

/// Land a change every `every` until `stop`, recording index lag.
async fn landings(
    writer: Arc<SearchIndex>,
    reader: Arc<SearchIndex>,
    trees: TreeOps,
    mut root: TreeHash,
    files: usize,
    every: Duration,
    stop: Instant,
    current: Current,
) -> anyhow::Result<Histogram<u64>> {
    let zipf = corpus::Zipf::default();
    let mut rng = StdRng::seed_from_u64(26);
    let mut lags = histogram();
    let mut added = 0;
    let mut rev = 1;
    while Instant::now() + every < stop {
        tokio::time::sleep(every).await;
        let mut builder = TreeBuilder::new();
        for _ in 0..rng.random_range(1..=20) {
            let i = if rng.random_range(0..4) == 0 {
                added += 1;
                files + added
            } else {
                rng.random_range(0..files)
            };
            builder.edit(
                &rel(&corpus::path(i, files)),
                inline(corpus::content(i, rev, &zipf)),
            );
        }
        rev += 1;
        let next = builder.build(&trees, Some(root)).await?;
        let landed = Instant::now();
        writer.index_root_from(next, Some(root), None).await?;
        writer.compact(None).await?;
        reader.refresh().await?;
        let view = reader
            .view(next)
            .await?
            .context("the reader sees the landing")?;
        lags.record(landed.elapsed().as_micros() as u64)?;
        *current.write() = Scope::indexed(view);
        root = next;
    }
    Ok(lags)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let (repo, trees, root, reused) = repository(&args).await?;

    let derive = DeriveManager::new(repo.clone(), trees.clone(), 64);
    let writer = SearchIndex::open(
        repo.clone(),
        derive.clone(),
        index_config(args.dir.join("writer-cache")),
    )
    .await?;
    let started = Instant::now();
    let stats = writer.index_root(root, None).await?;
    let index_secs = started.elapsed().as_secs_f64();
    eprintln!("indexed in {index_secs:.0}s: {stats:?}");

    let reader = SearchIndex::open(
        repo.clone(),
        derive.clone(),
        index_config(args.dir.join("reader-cache")),
    )
    .await?;
    let started = Instant::now();
    let view: Arc<PathView> = reader.view(root).await?.context("the root is indexed")?;
    let open_secs = started.elapsed().as_secs_f64();
    eprintln!("reader opened {} files in {open_secs:.1}s", view.len());
    let current: Current = Arc::new(RwLock::new(Scope::indexed(view)));

    let begin = Instant::now();
    let stop = begin + Duration::from_secs(args.duration);
    let lander = tokio::spawn(landings(
        writer.clone(),
        reader.clone(),
        trees.clone(),
        root,
        args.files,
        Duration::from_millis(args.land_every_ms),
        stop,
        current.clone(),
    ));

    let kinds: Arc<Mutex<BTreeMap<&'static str, Histogram<u64>>>> = Default::default();
    let all = Arc::new(Mutex::new(histogram()));
    let (in_flight, dropped, errors) = (
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicU64::new(0)),
    );
    let interval = Duration::from_secs_f64(1.0 / args.qps.max(1) as f64);
    let mut rng = StdRng::seed_from_u64(1);
    let mut next = begin;
    let mut tasks = tokio::task::JoinSet::new();
    while next < stop {
        let now = Instant::now();
        while next <= now && next < stop {
            next += interval;
            if in_flight.load(Ordering::Relaxed) >= args.max_in_flight {
                dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let (kind, query) = queries::next(&mut rng);
            let scope = current.read().clone();
            let (reader, kinds, all, in_flight, errors) = (
                reader.clone(),
                kinds.clone(),
                all.clone(),
                in_flight.clone(),
                errors.clone(),
            );
            in_flight.fetch_add(1, Ordering::Relaxed);
            tasks.spawn(async move {
                let t = Instant::now();
                let result =
                    rill_search::search(&reader, &scope, &query, |_: &RepoPath| true).await;
                let us = t.elapsed().as_micros() as u64;
                in_flight.fetch_sub(1, Ordering::Relaxed);
                if let Err(e) = result {
                    errors.fetch_add(1, Ordering::Relaxed);
                    eprintln!("query {:?} failed: {e}", query.pattern);
                    return;
                }
                let _ = all.lock().record(us);
                let _ = kinds
                    .lock()
                    .entry(kind)
                    .or_insert_with(histogram)
                    .record(us);
            });
        }
        while tasks.try_join_next().is_some() {}
        tokio::time::sleep_until(next.into()).await;
    }
    let offered_secs = begin.elapsed().as_secs_f64();
    while tasks.join_next().await.is_some() {}
    let lags = lander.await??;

    let all = all.lock().clone();
    let achieved = all.len() as f64 / offered_secs;
    let p99 = Duration::from_micros(all.value_at_quantile(0.99));
    let lag_p99 = Duration::from_micros(lags.value_at_quantile(0.99));
    let (dropped, errors) = (
        dropped.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    );
    let pass = achieved >= args.qps as f64 * 0.95
        && p99 < TARGET_P99
        && lags.len() > 0
        && lag_p99 < TARGET_LAG_P99
        && dropped == 0
        && errors == 0;
    let report = json!({
        "files": args.files,
        "reused_tree": reused,
        "index": {
            "seconds": index_secs,
            "already_indexed": stats.already_indexed,
            "blobs_indexed": stats.blobs_indexed,
            "segments_written": stats.segments_written,
            "reader_open_seconds": open_secs,
        },
        "queries": {
            "offered_qps": args.qps,
            "achieved_qps": achieved,
            "dropped": dropped,
            "errors": errors,
            "all": summary(&all),
            "by_kind": kinds.lock().iter().map(|(k, h)| (k.to_string(), summary(h))).collect::<BTreeMap<_, _>>(),
        },
        "index_lag": summary(&lags),
        "targets": {
            "qps": args.qps,
            "p99_ms": TARGET_P99.as_millis() as u64,
            "lag_p99_ms": TARGET_LAG_P99.as_millis() as u64,
        },
        "pass": pass,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !pass {
        std::process::exit(1);
    }
    Ok(())
}
