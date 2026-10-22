//! `trunk_red`: the self-healing gate. Breaks trunk of an embedded server
//! in five ways (a breaking change, two overlapping culprits, a flaky
//! test, a revert that conflicts, a forward fix) and checks each is
//! found, blamed and repaired within the SLOs.

use std::time::Duration;

use clap::Parser;
use rill_load::healing::{run_scenarios, Slo, Trunk};
use rill_load::EmbeddedServer;
use rill_sdk::{Client, ClientConfig};
use rill_types::VerifyMode;

#[derive(Parser)]
#[command(about = "Break trunk on purpose and time how it heals")]
struct Args {
    /// Seconds the server waits for a forward fix before reverting.
    #[arg(long, default_value_t = 30)]
    grace: u64,
    /// Seconds between post-land sweeps (the server default is 30).
    #[arg(long, default_value_t = 30)]
    interval: u64,
    /// In-process execution workers.
    #[arg(long, default_value_t = 8)]
    exec_workers: usize,
    /// Run only this scenario (`breaking change`, `overlapping culprits`,
    /// `flaky test`, `revert conflict`, `forward fix`).
    #[arg(long)]
    only: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let grace = Duration::from_secs(args.grace);
    let mut config = EmbeddedServer::config();
    config.verify.enabled = true;
    config.exec.local.workers = args.exec_workers;
    config.postland.enabled = true;
    config.postland.interval_secs = args.interval;
    config.lander.revert_grace = grace;
    // Breaks reach trunk only when pre-land verification misses them;
    // here it never runs, so post-land verification sees every one.
    let modes = &mut config.lander.modes;
    for mode in [
        &mut modes.trivial,
        &mut modes.low,
        &mut modes.normal,
        &mut modes.sensitive,
        &mut modes.critical,
    ] {
        *mode = VerifyMode::None;
    }
    let server = EmbeddedServer::start_with(config).await?;
    let client = Client::connect(ClientConfig::new(server.endpoint.clone())).await?;
    let markers = tempfile::tempdir()?;
    let trunk = Trunk::seed(client, "healing", &markers.path().display().to_string()).await?;
    let report = run_scenarios(&trunk, &Slo::with_grace(grace), args.only.as_deref()).await;
    println!("{}", serde_json::to_string_pretty(&report)?);
    anyhow::ensure!(report["passed"] == true, "the trunk-red gate failed");
    Ok(())
}
