//! `rill checks`: what verification ran, test by test and run by run.

mod runs;
mod tests;

use clap::Subcommand;

use crate::context::Ctx;
use crate::Result;

/// `rill checks <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ChecksCmd {
    /// Steps and tests whose outcome changes on unchanged inputs.
    Flaky(tests::FlakyArgs),
    /// Recent runs of one step (`step:<name>`) or test (`test:<id>`).
    History(tests::HistoryArgs),
    /// Recent verification runs.
    Runs(runs::RunsArgs),
    /// One run's steps; `--watch` follows it until it ends.
    Run(runs::RunArgs),
    /// A step's output (standard output, or `--stderr`).
    Log(runs::LogArgs),
    /// A file a step produced.
    Artifact(runs::ArtifactArgs),
}

/// Run a checks subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ChecksCmd) -> Result<()> {
    match cmd {
        ChecksCmd::Flaky(a) => tests::flaky(ctx, a).await,
        ChecksCmd::History(a) => tests::history(ctx, a).await,
        ChecksCmd::Runs(a) => runs::list(ctx, a).await,
        ChecksCmd::Run(a) => runs::show(ctx, a).await,
        ChecksCmd::Log(a) => runs::log(ctx, a).await,
        ChecksCmd::Artifact(a) => runs::artifact(ctx, a).await,
    }
}
