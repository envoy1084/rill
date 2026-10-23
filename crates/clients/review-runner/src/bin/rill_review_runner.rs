//! `rill-review-runner`: lease and run one agent's review or resolve jobs.

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use rill_review_runner::args::{Args, BackendKind, Mode};
use rill_review_runner::backend::{Backend, Claude, Fake};
use rill_review_runner::{resolve, review, Error, Result};
use rill_sdk::{Client, ClientConfig};

/// The variable holding the Claude API key.
const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

async fn run(args: Args) -> Result<()> {
    let mut config = ClientConfig::from_env()
        .or_else(|e| args.endpoint.clone().map(ClientConfig::new).ok_or(e))?;
    if let Some(endpoint) = &args.endpoint {
        config.endpoint = endpoint.clone();
    }
    config.user_agent = format!("rill-review-runner/{}", env!("CARGO_PKG_VERSION"));
    let client = Client::connect(config).await?;
    let backend: Arc<dyn Backend> = match args.backend {
        BackendKind::Fake => Arc::new(Fake {
            detect_permille: args.detect_permille,
            false_positive_permille: args.false_positive_permille,
        }),
        BackendKind::Claude => {
            let key = std::env::var(API_KEY_ENV)
                .map_err(|_| Error::Config(format!("{API_KEY_ENV} is not set")))?;
            Arc::new(Claude::new(&args.anthropic_url, key, args.max_tokens))
        }
    };
    let repo = client.repo(&args.repo);
    let tally = match args.mode {
        Mode::Review => {
            review::run(&repo.reviewers(), backend.as_ref(), args.wait_ms, args.once).await?
        }
        Mode::Resolve => resolve::run(&repo, backend.as_ref(), args.wait_ms, args.once).await?,
    };
    tracing::info!(done = tally.done, failed = tally.failed, "runner stopped");
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rill-review-runner: {e}");
            ExitCode::FAILURE
        }
    }
}
