//! The open-loop driver: each workload class fires at its target rate, and
//! work that cannot start because too much is in flight is counted as
//! dropped rather than silently delayed.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use rill_sdk::{ApplyOptions, Client, ClientConfig, Edit, EditBase, LandStatus, Root};
use rill_types::{FileKind, SparseProfile, Tree};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::ledger::Ledger;
use crate::profile::Profile;
use crate::stats::Stats;
use crate::workload::{file_path, Workload, DIRS, FILES_PER_DIR};

/// One load run's settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub endpoint: String,
    pub repo: String,
    pub profile: Profile,
    pub duration: Duration,
    pub max_in_flight: usize,
    pub hot_share: f64,
    /// Every new version writes a file of its own.
    pub disjoint: bool,
    /// Bearer token, when the cluster authenticates.
    pub token: Option<String>,
    /// Streams watching the event log.
    pub watchers: usize,
    /// Print an interim report to stderr this often; long runs.
    pub report_every: Option<Duration>,
}

/// Create and seed the repository, then drive load for the duration.
pub async fn run(settings: Settings) -> anyhow::Result<serde_json::Value> {
    let mut config = ClientConfig::new(settings.endpoint.clone());
    if let Some(token) = &settings.token {
        config = config.with_token(token.clone());
    }
    let client = Client::connect(config).await?;
    client
        .create_repo(&settings.repo, Some("load test".into()))
        .await?;
    let repo = client.repo(&settings.repo);
    seed(&repo).await?;

    let stats = Arc::new(Stats::default());
    let ledger = Arc::new(Ledger::default());
    let workload = Arc::new(Workload::new(
        repo.clone(),
        stats.clone(),
        ledger.clone(),
        settings.hot_share,
        settings.disjoint,
    ));
    let limit = Arc::new(Semaphore::new(settings.max_in_flight));
    let p = settings.profile;
    let deadline = Instant::now() + settings.duration;
    let started = Instant::now();

    let mut tickers = JoinSet::new();
    if settings.watchers > 0 {
        tickers.spawn(crate::watch::watch(
            repo.clone(),
            settings.watchers,
            deadline,
            stats.clone(),
        ));
    }
    if let Some(every) = settings.report_every {
        let stats = stats.clone();
        tickers.spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.tick().await;
            let until = tokio::time::Instant::from_std(deadline);
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    _ = tokio::time::sleep_until(until) => return,
                }
                let report = stats.report(started.elapsed());
                eprintln!("{}", serde_json::json!({ "interim": report }));
            }
        });
    }
    ticker(
        &mut tickers,
        "read",
        p.reads_per_sec,
        deadline,
        &limit,
        &stats,
        {
            let w = workload.clone();
            move || {
                let w = w.clone();
                async move { w.read().await }
            }
        },
    );
    ticker(
        &mut tickers,
        "create_version",
        p.versions_per_sec,
        deadline,
        &limit,
        &stats,
        {
            let w = workload.clone();
            move || {
                let w = w.clone();
                async move { w.new_version().await }
            }
        },
    );
    ticker(
        &mut tickers,
        "review",
        p.votes_per_sec,
        deadline,
        &limit,
        &stats,
        {
            let w = workload.clone();
            move || {
                let w = w.clone();
                async move { w.review().await }
            }
        },
    );
    ticker(
        &mut tickers,
        "land",
        p.lands_per_sec,
        deadline,
        &limit,
        &stats,
        {
            let w = workload.clone();
            move || {
                let w = w.clone();
                async move { w.land().await }
            }
        },
    );
    while let Some(result) = tickers.join_next().await {
        result?;
    }
    // Rates are over the load window; in-flight work finishing after it
    // still counts toward latencies.
    let mut report = stats.report(settings.duration);
    report["ledger"] = ledger.verify(&repo).await;
    report["slo"] = crate::slo::judge(&report);
    report["drain_secs"] =
        serde_json::json!((started.elapsed().saturating_sub(settings.duration)).as_secs_f64());
    report["target"] = serde_json::json!({
        "percent_of_d1": p.percent,
        "reads_per_sec": p.reads_per_sec,
        "versions_per_sec": p.versions_per_sec,
        "votes_per_sec": p.votes_per_sec,
        "lands_per_sec": p.lands_per_sec,
    });
    Ok(report)
}

fn ticker<F, Fut>(
    set: &mut JoinSet<()>,
    op: &'static str,
    per_sec: f64,
    deadline: Instant,
    limit: &Arc<Semaphore>,
    stats: &Arc<Stats>,
    make: F,
) where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    if per_sec <= 0.0 {
        return;
    }
    let limit = limit.clone();
    let stats = stats.clone();
    let period = Duration::from_secs_f64(1.0 / per_sec);
    set.spawn(async move {
        let mut tick = tokio::time::interval(period);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        let mut work = JoinSet::new();
        let deadline = tokio::time::Instant::from_std(deadline);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = tokio::time::sleep_until(deadline) => break,
            }
            match limit.clone().try_acquire_owned() {
                Ok(permit) => {
                    let fut = make();
                    work.spawn(async move {
                        fut.await;
                        drop(permit);
                    });
                }
                Err(_) => stats.dropped(op),
            }
            while work.try_join_next().is_some() {}
        }
        while work.join_next().await.is_some() {}
    });
}

/// Land one change containing every seeded file.
async fn seed(repo: &rill_sdk::RepoClient) -> anyhow::Result<()> {
    // A new repository has no trunk; creating a workspace stores the empty
    // tree the first change is based on.
    repo.workspaces()
        .create("load-seed", None, SparseProfile::full())
        .await?;
    let mut edits = Vec::with_capacity(DIRS * FILES_PER_DIR);
    for dir in 0..DIRS {
        for file in 0..FILES_PER_DIR {
            edits.push(Edit::Set {
                path: rill_types::RepoPathBuf::new(file_path(dir, file))?,
                bytes: Bytes::from(format!("pub const value_{}: u32 = 0;\n", dir * 100 + file)),
                kind: FileKind::Regular,
            });
        }
    }
    let result = repo
        .apply_edits(
            EditBase::Root(Root::Tree(Tree::empty_hash())),
            edits,
            ApplyOptions::message("seed"),
        )
        .await?;
    let change = result.version.change_id;
    let reviews = repo.reviews();
    reviews.submit(change).await?;
    reviews.vote(change, "Review", 2, None, None).await?;
    reviews.vote(change, "Verified", 1, None, None).await?;
    // The seed is large enough to be high risk, which needs an owner too.
    reviews.vote(change, "Owner", 1, None, None).await?;
    let ticket = reviews.request_land(change).await?;
    let landed = repo
        .stack()
        .await_land(&ticket.id, Duration::from_secs(60))
        .await?;
    anyhow::ensure!(landed.status() == LandStatus::Landed, "seed did not land");
    Ok(())
}
