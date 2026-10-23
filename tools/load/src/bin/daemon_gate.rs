//! `daemon_gate`: the phase 20 exit gate for the client daemon.
//!
//! Builds a synthetic repository of `dirs × files_per_dir` files (10M by
//! default) on an in-process server: leaf directories come from a small
//! set of distinct trees, so the repository is as large as asked while
//! storage stays small, the way content addressing deduplicates. Then:
//!
//! * clone: a lazy mount (Linux) to a usable shell (list, stat, read);
//! * status: p99 of `GetStatus` with `edits` local edits;
//! * CI: a build reading every file of one directory from a cold host
//!   cache, through a regional `rill-cache` another host already warmed;
//!   the regional hit rate of the cold host's reads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use bytes::Bytes;
use clap::Parser;
use rill_daemon::proto as dpb;
use rill_daemon::{Daemon, DaemonClient, DaemonConfig, DaemonDir};
use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Client, Edit, EditBase, Root};
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, ContentRef, FileKind, Principal, Provenance,
    RepoPathBuf, SubtreeChange, Timestamp,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(about = "Phase 20 exit gate: lazy clone, status latency, CI cache hit rate")]
struct Args {
    /// Leaf directories.
    #[arg(long, default_value_t = 10_000)]
    dirs: usize,
    /// Files in each leaf directory.
    #[arg(long, default_value_t = 1_000)]
    files_per_dir: usize,
    /// Distinct leaf trees the directories are drawn from.
    #[arg(long, default_value_t = 100)]
    variants: usize,
    /// Local edits status runs against.
    #[arg(long, default_value_t = 1_000)]
    edits: usize,
    /// `GetStatus` calls timed.
    #[arg(long, default_value_t = 200)]
    status_runs: usize,
}

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).expect("generated paths are valid")
}

fn leaf(k: usize) -> String {
    format!("t/{k:03}")
}

/// Leaf directory `i`, grouped a hundred to a directory.
fn dir(i: usize) -> String {
    format!("d/{:03}/{:02}", i / 100, i % 100)
}

fn file(n: usize) -> String {
    format!("f{n:04}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let server = DevServer::start().await?;
    let client = server.client().await?;
    let started = Instant::now();
    let version = build_repo(&client, &args).await?;
    let built = started.elapsed();
    eprintln!(
        "built {} files in {:.1}s",
        args.dirs * args.files_per_dir,
        built.as_secs_f64()
    );

    let scratch = tempfile::tempdir()?;
    let (clone, status) = clone_and_status(&server, version, &args, scratch.path()).await?;
    let ci = ci_build(&server, version, scratch.path()).await?;

    let report = json!({
        "repo_files": args.dirs * args.files_per_dir,
        "build_secs": built.as_secs_f64(),
        "clone": clone,
        "status": status,
        "ci": ci,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    let mut failed = Vec::new();
    if let Some(secs) = report["clone"]["usable_secs"].as_f64() {
        if secs >= 5.0 {
            failed.push(format!("lazy clone took {secs:.2}s (gate < 5s)"));
        }
    }
    let p99 = report["status"]["p99_ms"].as_f64().unwrap_or(f64::MAX);
    if p99 >= 200.0 {
        failed.push(format!("status p99 {p99:.1} ms (gate < 200 ms)"));
    }
    let rate = report["ci"]["regional_hit_rate"].as_f64().unwrap_or(0.0);
    if rate < 0.99 {
        failed.push(format!("regional hit rate {rate:.3} (gate ≥ 0.99)"));
    }
    if !failed.is_empty() {
        bail!("gate failed: {}", failed.join("; "));
    }
    eprintln!("gate passed");
    Ok(())
}

/// Leaf variants under `t/` (variant 0 with distinct blobs, the CI build
/// set), then a version copying them into `d/` and dropping `t/`.
async fn build_repo(client: &Client, args: &Args) -> anyhow::Result<ChangeVersionHash> {
    client.create_repo("big", None).await?;
    let repo = client.repo("big");
    let mut shared = Vec::new();
    for i in 0..64 {
        shared.push(
            repo.put_blob(Bytes::from(format!("shared {i}\n").repeat(40)))
                .await?,
        );
    }
    let mut unique = Vec::with_capacity(args.files_per_dir);
    for n in 0..args.files_per_dir {
        unique.push(
            repo.put_blob(Bytes::from(format!("unique {n}\n").repeat(40)))
                .await?,
        );
    }
    let content = |k: usize, n: usize| -> ContentRef {
        if k == 0 {
            unique[n].clone()
        } else {
            shared[(k * 7 + n) % shared.len()].clone()
        }
    };
    let mut base = Root::trunk();
    let mut edits = Vec::new();
    let mut last = None;
    for k in 0..args.variants {
        for n in 0..args.files_per_dir {
            edits.push(Edit::Stored {
                path: path(&format!("{}/{}", leaf(k), file(n))),
                content: content(k, n),
                kind: FileKind::Regular,
            });
        }
        if edits.len() >= 5_000 || k + 1 == args.variants {
            let made = repo
                .apply_edits(
                    EditBase::Root(base.clone()),
                    std::mem::take(&mut edits),
                    ApplyOptions::message("leaves"),
                )
                .await?;
            base = Root::Version(made.hash);
            last = Some(made.hash);
        }
    }
    let leaves = last.context("no leaf variants")?;
    // A version may copy at most 100k files: one version per group of
    // leaf directories, the last also dropping `t/`.
    let per_version = (rill_types::limits::MAX_FILES_PER_CHANGE / args.files_per_dir).clamp(1, 100);
    let mut parent = leaves;
    let groups: Vec<Vec<usize>> = (0..args.dirs)
        .collect::<Vec<_>>()
        .chunks(per_version)
        .map(<[usize]>::to_vec)
        .collect();
    for (g, group) in groups.iter().enumerate() {
        let mut subtrees = BTreeMap::new();
        for &i in group {
            subtrees.insert(
                path(&dir(i)),
                SubtreeChange::Copy {
                    from_path: path(&leaf(i % args.variants)),
                    from_version: leaves,
                },
            );
        }
        if g + 1 == groups.len() {
            subtrees.insert(path("t"), SubtreeChange::Delete);
        }
        let version = ChangeVersion {
            change_id: ChangeId::new(),
            parents: vec![parent],
            predecessor: None,
            subtree_changes: subtrees,
            file_changes: BTreeMap::new(),
            author: Principal::human("dev"),
            provenance: Provenance::default(),
            intent: None,
            message: format!("synthetic group {g}"),
            created_at: Timestamp::now(),
            extra: BTreeMap::new(),
        };
        parent = repo.create_change_version(&version, vec![]).await?;
    }
    Ok(parent)
}

struct Host {
    daemon: Arc<Daemon>,
    rpc: DaemonClient,
}

async fn host(root: &Path, cache_url: Option<String>) -> anyhow::Result<Host> {
    let dir = DaemonDir::new(root);
    let mut config = DaemonConfig::new(dir.clone());
    config.cache_budget = 8 << 30;
    config.clients = Arc::new(move |endpoint, token| {
        let mut c = rill_sdk::ClientConfig::new(endpoint);
        c.token = token;
        c.cache_url = cache_url.clone();
        c
    });
    let daemon = Daemon::open(config).await?;
    tokio::spawn(rill_daemon::serve(daemon.clone()));
    for _ in 0..200 {
        if let Ok(rpc) = DaemonClient::connect(&dir).await {
            return Ok(Host { daemon, rpc });
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bail!("daemon at {} did not start", root.display())
}

async fn mount(
    host: &Host,
    server: &DevServer,
    version: ChangeVersionHash,
    at: &Path,
    mode: dpb::MountMode,
    sparse: Vec<String>,
) -> anyhow::Result<(String, u64)> {
    let created = host
        .rpc
        .create_mount(dpb::CreateMountRequest {
            path: at.display().to_string(),
            endpoint: server.endpoint().to_string(),
            repo: "big".into(),
            revision: version.to_hex(),
            sparse,
            mode: mode.into(),
            ..Default::default()
        })
        .await?;
    Ok((
        created.mount.into_option().unwrap_or_default().name,
        created.files,
    ))
}

fn lazy_supported() -> bool {
    (cfg!(target_os = "linux") && Path::new("/dev/fuse").exists())
        || (cfg!(target_os = "macos") && Path::new("/sbin/mount_nfs").exists())
}

/// The lazy clone to a usable shell (Linux), then status latency with
/// local edits (on the lazy mount, or an eager sparse one elsewhere).
async fn clone_and_status(
    server: &DevServer,
    version: ChangeVersionHash,
    args: &Args,
    scratch: &Path,
) -> anyhow::Result<(serde_json::Value, serde_json::Value)> {
    let host = host(&scratch.join("host-dev"), None).await?;
    let work = scratch.join("work");
    let (name, clone) = if lazy_supported() {
        let started = Instant::now();
        let (name, _) = mount(&host, server, version, &work, dpb::MountMode::Lazy, vec![]).await?;
        let mounted = started.elapsed();
        let top: Vec<_> = std::fs::read_dir(&work)?.collect();
        let dirs = std::fs::read_dir(work.join("d"))?.count();
        let groups = args.dirs.div_ceil(100);
        let probe = work
            .join(dir(args.dirs / 2))
            .join(file(args.files_per_dir / 2));
        let size = std::fs::metadata(&probe)?.len();
        let read = std::fs::read(&probe)?.len() as u64;
        let usable = started.elapsed();
        anyhow::ensure!(
            size == read && !top.is_empty() && dirs == groups,
            "the view is wrong: stat {size} read {read}, {} top entries, {dirs} of {groups} groups",
            top.len()
        );
        (
            name,
            json!({
                "mode": "lazy",
                "mount_secs": mounted.as_secs_f64(),
                "usable_secs": usable.as_secs_f64(),
                "listed_groups": dirs,
            }),
        )
    } else {
        let started = Instant::now();
        let (name, files) = mount(
            &host,
            server,
            version,
            &work,
            dpb::MountMode::Eager,
            vec![dir(0)],
        )
        .await?;
        (
            name,
            json!({
                "mode": "eager sparse (no FUSE here; the lazy clone gate needs Linux)",
                "files": files,
                "checkout_secs": started.elapsed().as_secs_f64(),
            }),
        )
    };

    let edit_dir = work.join(dir(0));
    for n in 0..args.edits.min(args.files_per_dir) {
        std::fs::write(edit_dir.join(file(n)), format!("edited {n}\n"))?;
    }
    for n in args.files_per_dir..args.edits {
        std::fs::write(edit_dir.join(format!("new{n}")), "new\n")?;
    }
    // The first status waits for the watcher to reconcile the edits just
    // made (eager mounts); the gate times status once they are recorded.
    let warmup = Instant::now();
    host.rpc
        .get_status(dpb::GetStatusRequest {
            name: name.clone(),
            prefixes: vec![],
        })
        .await?;
    let first_ms = warmup.elapsed().as_secs_f64() * 1e3;
    let mut samples = Vec::with_capacity(args.status_runs);
    let mut changes = 0;
    for _ in 0..args.status_runs {
        let started = Instant::now();
        let reply = host
            .rpc
            .get_status(dpb::GetStatusRequest {
                name: name.clone(),
                prefixes: vec![],
            })
            .await?;
        samples.push(started.elapsed());
        changes = reply.changes.len();
    }
    anyhow::ensure!(
        changes == args.edits,
        "status saw {changes} of {} edits",
        args.edits
    );
    samples.sort();
    let ms = |q: f64| {
        samples[((samples.len() as f64 * q) as usize).min(samples.len() - 1)].as_secs_f64() * 1e3
    };
    let status = json!({
        "edits": changes,
        "first_ms": first_ms,
        "runs": samples.len(),
        "p50_ms": ms(0.50),
        "p99_ms": ms(0.99),
        "max_ms": samples.last().map(|d| d.as_secs_f64() * 1e3),
    });
    host.daemon.stop();
    Ok((clone, status))
}

/// One host warms the regional cache with a build; a second host with a
/// cold local cache runs the same build. Returns the second host's
/// regional hit rate.
async fn ci_build(
    server: &DevServer,
    version: ChangeVersionHash,
    scratch: &Path,
) -> anyhow::Result<serde_json::Value> {
    let dir_path = scratch.join("regional");
    let mut config = rill_cache::CacheConfig::new(server.http(), &dir_path, 4 << 30);
    config.listen = "127.0.0.1:0".parse()?;
    let cache = rill_cache::Cache::start(&config).await?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let url = format!("http://{}", listener.local_addr()?);
    let stop = CancellationToken::new();
    tokio::spawn({
        let (cache, config, stop) = (cache.clone(), config.clone(), stop.clone());
        async move { rill_cache::serve_on(cache, listener, &config, stop).await }
    });

    let stats = cache.stats();
    let counts = || {
        (
            stats.hits.load(Ordering::Relaxed),
            stats.misses.load(Ordering::Relaxed),
        )
    };
    let mode = if lazy_supported() {
        dpb::MountMode::Snapshot
    } else {
        dpb::MountMode::Eager
    };
    let warm = build_once(server, version, scratch, "warm", &url, mode).await?;
    let before = counts();
    let started = Instant::now();
    let cold = build_once(server, version, scratch, "cold", &url, mode).await?;
    let elapsed = started.elapsed();
    let after = counts();
    stop.cancel();
    let (hits, misses) = (after.0 - before.0, after.1 - before.1);
    let total = (hits + misses).max(1);
    Ok(json!({
        "mode": if mode == dpb::MountMode::Snapshot { "snapshot" } else { "eager sparse" },
        "files_read": cold,
        "warm_files_read": warm,
        "cold_host_secs": elapsed.as_secs_f64(),
        "regional_hits": hits,
        "regional_misses": misses,
        "regional_hit_rate": hits as f64 / total as f64,
    }))
}

/// Check out `d/00000` on a fresh host and read every file in it.
async fn build_once(
    server: &DevServer,
    version: ChangeVersionHash,
    scratch: &Path,
    name: &str,
    cache_url: &str,
    mode: dpb::MountMode,
) -> anyhow::Result<usize> {
    let host = host(
        &scratch.join(format!("host-{name}")),
        Some(cache_url.to_string()),
    )
    .await?;
    let at: PathBuf = scratch.join(format!("ci-{name}"));
    let (mount_name, _) = mount(&host, server, version, &at, mode, vec![dir(0)]).await?;
    let mut read = 0;
    for entry in std::fs::read_dir(at.join(dir(0)))? {
        let entry = entry?;
        std::fs::read(entry.path())?;
        read += 1;
    }
    host.rpc
        .delete_mount(dpb::DeleteMountRequest { name: mount_name })
        .await?;
    host.daemon.stop();
    Ok(read)
}
