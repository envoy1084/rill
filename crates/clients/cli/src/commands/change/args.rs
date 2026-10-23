//! Arguments of the working-change commands.

use clap::Args;

/// `rill new`.
#[derive(Args, Debug)]
pub struct NewArgs {
    /// Change message (default: write it in your editor).
    #[arg(short, long)]
    pub message: Option<String>,
    /// Intent the change belongs to.
    #[arg(long)]
    pub intent: Option<String>,
}

/// `rill describe`.
#[derive(Args, Debug)]
pub struct DescribeArgs {
    /// New change message (default: edit the current one in your editor).
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill amend`.
#[derive(Args, Debug)]
pub struct AmendArgs {
    /// Replace the message too (default: keep it).
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill sync`.
#[derive(Args, Debug)]
pub struct SyncArgs {
    /// Root to move the base to (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub onto: Option<String>,
}

/// `rill resolve`.
#[derive(Args, Debug)]
pub struct ResolveArgs {
    /// Mark every conflicted path resolved.
    #[arg(long)]
    pub all: bool,
    /// Walk each conflict and pick a side (all conflicted paths when none
    /// are named).
    #[arg(long, short = 'i', conflicts_with_all = ["all", "force"])]
    pub interactive: bool,
    /// Skip the check that no conflict markers remain.
    #[arg(long)]
    pub force: bool,
    /// Paths to mark resolved.
    pub paths: Vec<String>,
}
