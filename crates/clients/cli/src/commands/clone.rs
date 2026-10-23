//! `rill clone`: check out a repository into a directory named after it,
//! lazily where the platform has a virtual filesystem.

use std::path::PathBuf;

use clap::Args;

use super::checkout::{self, CheckoutArgs};
use crate::context::Ctx;
use crate::Result;

/// `rill clone`.
#[derive(Args, Debug)]
pub struct CloneArgs {
    /// Repository name or id.
    pub repo: String,
    /// Directory to create (default: the repository's name).
    pub dir: Option<PathBuf>,
    /// Revision: trunk, trunk@N, a version hash or a ref.
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Only these directory prefixes (repeatable).
    #[arg(long, value_name = "PREFIX")]
    pub sparse: Vec<String>,
    /// Write files to disk instead of the virtual filesystem.
    #[arg(long)]
    pub eager: bool,
}

/// Run `rill clone`.
pub async fn run(ctx: &mut Ctx<'_>, args: CloneArgs) -> Result<()> {
    let name = args
        .repo
        .rsplit('/')
        .next()
        .unwrap_or(&args.repo)
        .to_string();
    let lazy = !args.eager && cfg!(any(target_os = "linux", target_os = "macos"));
    checkout::run(
        ctx,
        CheckoutArgs {
            repo: args.repo,
            dir: args.dir.unwrap_or_else(|| PathBuf::from(name)),
            at: args.at,
            sparse: args.sparse,
            no_workspace: false,
            lazy,
            snapshot: false,
        },
    )
    .await
}
