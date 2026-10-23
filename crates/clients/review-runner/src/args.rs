//! Command-line arguments.

use clap::{Parser, ValueEnum};

/// What the runner does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// Review changes routed to the agent.
    Review,
    /// Resolve conflicts dispatched to the agent.
    Resolve,
}

/// Which backend runs the jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BackendKind {
    /// Deterministic, offline: flags seeded `RILL-DEFECT` markers.
    Fake,
    /// The Claude Messages API.
    Claude,
}

/// `rill-review-runner`: lease and run one agent's jobs.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Args {
    /// The repository (`org/repo`).
    #[arg(long, env = "RILL_REPO")]
    pub repo: String,
    /// The server; default `RILL_ENDPOINT`.
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long, value_enum, default_value_t = Mode::Review)]
    pub mode: Mode,
    #[arg(long, value_enum, default_value_t = BackendKind::Fake)]
    pub backend: BackendKind,
    /// Stop when no job is waiting instead of waiting for more.
    #[arg(long)]
    pub once: bool,
    /// How long each lease request waits for a job, in milliseconds.
    #[arg(long, default_value_t = 20_000)]
    pub wait_ms: u32,
    /// Fake backend: seeded defects caught per thousand.
    #[arg(long, default_value_t = 950)]
    pub detect_permille: u32,
    /// Fake backend: clean changes wrongly blocked per thousand.
    #[arg(long, default_value_t = 0)]
    pub false_positive_permille: u32,
    /// Claude backend: the API's base URL.
    #[arg(
        long,
        env = "ANTHROPIC_BASE_URL",
        default_value = "https://api.anthropic.com"
    )]
    pub anthropic_url: String,
    /// Claude backend: the largest answer, in tokens.
    #[arg(long, default_value_t = 8192)]
    pub max_tokens: u32,
}
