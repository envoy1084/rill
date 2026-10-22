//! `cache_load`: read load through a `rill cache` in front of an origin.
//! Seeds files in directories, lists each directory for its read grant,
//! checks that 100 simultaneous misses on one object cost the origin one
//! fetch, then reads hot and cold files through the cache for a while and
//! reports latency and the cache's own counters.

use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use rand::RngExt;
use rill_proto::rill::v1 as pb;
use rill_sdk::{ApplyOptions, Client, ClientConfig, Edit, EditBase, Root};
use rill_types::{ObjectHash, ObjectKind, RepoPathBuf};

#[derive(Parser)]
#[command(about = "Read load through the cache tier")]
struct Args {
    /// The origin's gRPC endpoint, for seeding and listings.
    #[arg(long)]
    grpc: String,
    /// The cache's HTTP address.
    #[arg(long)]
    cache: String,
    #[arg(long, env = "RILL_TOKEN")]
    token: String,
    #[arg(long, default_value = "cache-load")]
    repo: String,
    #[arg(long, default_value_t = 10)]
    dirs: usize,
    #[arg(long, default_value_t = 100)]
    files_per_dir: usize,
    #[arg(long, default_value_t = 8192)]
    file_bytes: usize,
    #[arg(long, default_value_t = 64)]
    workers: usize,
    #[arg(long, default_value_t = 300)]
    seconds: u64,
}

fn content(dir: usize, file: usize, size: usize) -> Vec<u8> {
    let line = format!("dir {dir} file {file}\n");
    line.as_bytes().iter().copied().cycle().take(size).collect()
}

async fn metric(http: &reqwest::Client, cache: &str, name: &str) -> u64 {
    let text = http
        .get(format!("{cache}/metrics"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{name} ")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut config = ClientConfig::new(args.grpc.clone());
    config.token = Some(args.token.clone());
    let client = Client::connect(config).await.expect("connect");
    let _ = client.create_repo(&args.repo, None).await;
    let start = client
        .change_svc()
        .apply_edits(pb::ApplyEditsRequest {
            parent: rill_sdk::names::repo(&args.repo),
            message: "start".into(),
            ..Default::default()
        })
        .await
        .expect("start")
        .into_owned()
        .info
        .into_option()
        .expect("info")
        .hash;
    let mut edits = Vec::new();
    for d in 0..args.dirs {
        for f in 0..args.files_per_dir {
            edits.push(Edit::set(
                RepoPathBuf::new(format!("d{d}/f{f}.txt")).unwrap(),
                content(d, f, args.file_bytes),
            ));
        }
    }
    let seeded = Instant::now();
    let version = client
        .repo(&args.repo)
        .apply_edits(
            EditBase::Root(Root::Version(ObjectHash::from_slice(&start).unwrap())),
            edits,
            ApplyOptions {
                message: "seed".into(),
                ..Default::default()
            },
        )
        .await
        .expect("seed")
        .hash;
    eprintln!(
        "seeded {} files in {:?}",
        args.dirs * args.files_per_dir,
        seeded.elapsed()
    );

    // One listing (and grant) per directory.
    let mut targets: Vec<(String, Vec<ObjectHash>)> = Vec::new();
    for d in 0..args.dirs {
        let listing = client
            .content_svc()
            .list_directory(pb::ListDirectoryRequest {
                repo: rill_sdk::names::repo(&args.repo),
                revision: version.to_hex(),
                path: format!("d{d}"),
                ..Default::default()
            })
            .await
            .expect("list")
            .into_owned();
        let hashes = (0..args.files_per_dir)
            .map(|f| ObjectHash::digest(ObjectKind::Blob, &content(d, f, args.file_bytes)))
            .collect();
        targets.push((listing.read_grant, hashes));
    }
    let targets = Arc::new(targets);
    let http = reqwest::Client::new();
    let fetch = {
        let (http, cache, repo, token) = (
            http.clone(),
            args.cache.clone(),
            args.repo.clone(),
            args.token.clone(),
        );
        move |grant: String, hash: ObjectHash| {
            let (http, cache, repo, token) =
                (http.clone(), cache.clone(), repo.clone(), token.clone());
            async move {
                let started = Instant::now();
                let status = http
                    .get(format!("{cache}/o/{repo}/blob/{}", hash.to_hex()))
                    .bearer_auth(token)
                    .header("x-rill-grant", grant)
                    .send()
                    .await
                    .map(|r| r.status().as_u16())
                    .unwrap_or(0);
                (status, started.elapsed())
            }
        }
    };

    // Coalescing: 100 simultaneous misses on one cold object.
    let before = metric(&http, &args.cache, "rill_cache_origin_fetches_total").await;
    let (grant, hashes) = targets[0].clone();
    let cold = hashes[0];
    let burst: Vec<_> = (0..100)
        .map(|_| tokio::spawn(fetch(grant.clone(), cold)))
        .collect();
    let mut ok = 0;
    for b in burst {
        if b.await.unwrap().0 == 200 {
            ok += 1;
        }
    }
    let after = metric(&http, &args.cache, "rill_cache_origin_fetches_total").await;
    println!(
        "100 simultaneous misses: {ok} served, {} origin fetch(es)",
        after - before
    );

    // Mixed load: 80% of reads on the first tenth of the files.
    let deadline = Instant::now() + Duration::from_secs(args.seconds);
    let workers: Vec<_> = (0..args.workers)
        .map(|_| {
            let (targets, fetch) = (targets.clone(), fetch.clone());
            tokio::spawn(async move {
                let mut latencies = Vec::new();
                let mut errors = 0u64;
                while Instant::now() < deadline {
                    let (d, f) = {
                        let mut rng = rand::rng();
                        let hot = rng.random_bool(0.8);
                        let dirs = targets.len();
                        let files = targets[0].1.len();
                        if hot {
                            (
                                rng.random_range(0..dirs.div_ceil(10)),
                                rng.random_range(0..files),
                            )
                        } else {
                            (rng.random_range(0..dirs), rng.random_range(0..files))
                        }
                    };
                    let (grant, hashes) = &targets[d];
                    let (status, took) = fetch(grant.clone(), hashes[f]).await;
                    if status == 200 {
                        latencies.push(took);
                    } else {
                        errors += 1;
                    }
                }
                (latencies, errors)
            })
        })
        .collect();
    let mut all = Vec::new();
    let mut errors = 0;
    for w in workers {
        let (l, e) = w.await.unwrap();
        all.extend(l);
        errors += e;
    }
    all.sort();
    let pct = |p: f64| all[((all.len() - 1) as f64 * p) as usize];
    let hits = metric(&http, &args.cache, "rill_cache_hits_total").await;
    let misses = metric(&http, &args.cache, "rill_cache_misses_total").await;
    let evictions = metric(&http, &args.cache, "rill_cache_evictions_total").await;
    println!(
        "{} reads in {}s ({:.0}/s), {errors} errors; p50 {:?}, p99 {:?}, max {:?}",
        all.len(),
        args.seconds,
        all.len() as f64 / args.seconds as f64,
        pct(0.5),
        pct(0.99),
        all.last().unwrap()
    );
    println!(
        "cache: {hits} hits, {misses} misses ({:.1}% hit), {evictions} evictions",
        100.0 * hits as f64 / (hits + misses).max(1) as f64
    );
}
