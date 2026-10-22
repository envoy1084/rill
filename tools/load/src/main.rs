//! `rill-load`: drive a Rill server with synthetic agents.

use std::time::Duration;

use clap::Parser;
use rill_load::{run, Burst, EmbeddedServer, Profile, Settings};

#[derive(Parser)]
#[command(about = "Load generator: synthetic agents at a fraction of the D1 workload")]
struct Args {
    /// gRPC endpoint; an embedded dev server is started when omitted.
    #[arg(long)]
    endpoint: Option<String>,
    /// Repository to create and load.
    #[arg(long, default_value = "load")]
    repo: String,
    /// Percent of the D1 design workload.
    #[arg(long, default_value_t = 1.0)]
    percent: f64,
    /// Seconds to run.
    #[arg(long, default_value_t = 60)]
    duration: u64,
    /// Most operations in flight before new ones count as dropped.
    #[arg(long, default_value_t = 1024)]
    max_in_flight: usize,
    /// Share of edits and reads that hit the few hot files.
    #[arg(long, default_value_t = 0.3)]
    hot_share: f64,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
    /// Every new version writes a file of its own, so landings never
    /// interact (the sequencer's throughput gate).
    #[arg(long)]
    disjoint: bool,
    /// Landings per second, overriding the D1 share (versions and votes
    /// rise to feed them).
    #[arg(long)]
    lands_per_sec: Option<f64>,
    /// Bearer token for an authenticating cluster.
    #[arg(long, env = "RILL_TOKEN")]
    token: Option<String>,
    /// Streams watching the event log (D1 has 50,000).
    #[arg(long, default_value_t = 0)]
    watchers: usize,
    /// Print an interim report to stderr every this many seconds.
    #[arg(long)]
    report_every: Option<u64>,
    /// Changes contend for hot registries (merged by union) and a
    /// lockfile (regenerated); the embedded server runs workers for it.
    #[arg(long)]
    contended: bool,
    /// Land at twice the rate for --burst-secs of every this many seconds.
    #[arg(long, requires = "burst_secs")]
    burst_every: Option<u64>,
    #[arg(long)]
    burst_secs: Option<u64>,
    /// In-process execution workers of the embedded server.
    #[arg(long, default_value_t = 16)]
    exec_workers: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let embedded = match &args.endpoint {
        Some(_) => None,
        None => {
            Some(EmbeddedServer::start(if args.contended { args.exec_workers } else { 0 }).await?)
        }
    };
    let endpoint = args
        .endpoint
        .clone()
        .or_else(|| embedded.as_ref().map(|s| s.endpoint.clone()))
        .expect("one of the two is set");
    let burst = args
        .burst_every
        .zip(args.burst_secs)
        .map(|(every, length)| Burst {
            every: Duration::from_secs(every),
            length: Duration::from_secs(length),
        });
    let report = run(Settings {
        endpoint,
        repo: args.repo,
        profile: match (args.lands_per_sec, burst.is_some()) {
            (Some(lands), true) => Profile::of_d1(args.percent)
                .fed_for(lands * 2.0)
                .with_lands_only(lands),
            (Some(lands), false) => Profile::of_d1(args.percent).with_lands(lands),
            (None, _) => Profile::of_d1(args.percent),
        },
        duration: Duration::from_secs(args.duration),
        max_in_flight: args.max_in_flight,
        hot_share: args.hot_share,
        disjoint: args.disjoint,
        token: args.token,
        watchers: args.watchers,
        report_every: args.report_every.map(Duration::from_secs),
        contended: args.contended,
        burst,
    })
    .await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    print_table(&report);
    let ledger = &report["ledger"];
    println!(
        "acknowledged: {} versions, {} landings; lost: {} versions, {} landings",
        ledger["acknowledged_versions"],
        ledger["acknowledged_landings"],
        ledger["lost_versions"].as_array().map_or(0, Vec::len),
        ledger["lost_landings"].as_array().map_or(0, Vec::len),
    );
    if !report["resolutions"].is_null() {
        println!("merged by: {}", report["resolutions"]);
    }
    println!("SLOs met: {}", report["slo"]["met"]);
    Ok(())
}

fn print_table(report: &serde_json::Value) {
    println!(
        "{:.1}% of D1 for {:.0}s",
        report["target"]["percent_of_d1"].as_f64().unwrap_or(0.0),
        report["elapsed_secs"].as_f64().unwrap_or(0.0)
    );
    println!(
        "{:<16} {:>8} {:>7} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "op", "ok", "errors", "dropped", "per_sec", "p50_ms", "p90_ms", "p99_ms", "max_ms"
    );
    for row in report["ops"].as_array().into_iter().flatten() {
        println!(
            "{:<16} {:>8} {:>7} {:>8} {:>9.1} {:>9.2} {:>9.2} {:>9.2} {:>9.2}",
            row["op"].as_str().unwrap_or(""),
            row["ok"].as_u64().unwrap_or(0),
            row["errors"].as_u64().unwrap_or(0),
            row["dropped"].as_u64().unwrap_or(0),
            row["per_sec"].as_f64().unwrap_or(0.0),
            row["p50_ms"].as_f64().unwrap_or(0.0),
            row["p90_ms"].as_f64().unwrap_or(0.0),
            row["p99_ms"].as_f64().unwrap_or(0.0),
            row["max_ms"].as_f64().unwrap_or(0.0),
        );
    }
}
