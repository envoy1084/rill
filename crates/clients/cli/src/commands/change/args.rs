//! Arguments of the working-change commands.

use clap::Args;

/// `rill new`.
#[derive(Args, Debug)]
pub struct NewArgs {
    /// Change message.
    #[arg(short, long)]
    pub message: String,
    /// Intent the change belongs to.
    #[arg(long)]
    pub intent: Option<String>,
}

/// `rill describe`.
#[derive(Args, Debug)]
pub struct DescribeArgs {
    /// New change message.
    #[arg(short, long)]
    pub message: String,
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
    /// List conflicted paths instead of marking.
    #[arg(long)]
    pub list: bool,
    /// Mark the paths resolved (the default action).
    #[arg(long)]
    pub mark: bool,
    /// Mark every conflicted path resolved.
    #[arg(long)]
    pub all: bool,
    /// Skip the check that no conflict markers remain.
    #[arg(long)]
    pub force: bool,
    /// Paths to mark resolved.
    pub paths: Vec<String>,
}
