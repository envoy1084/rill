//! Which point in history a read addresses.

use rill_sdk::{RepoClient, Root};

use super::args::Args;
use super::context::ToolContext;
use crate::error::ToolResult;

/// `at` when given; else the current version of `change`, or of the
/// session's change; else trunk. Reads see the agent's own edits by
/// default.
pub async fn read_root(ctx: &ToolContext, repo: &RepoClient, args: &Args) -> ToolResult<Root> {
    if let Some(root) = args.opt_parse::<Root>("at")? {
        return Ok(root);
    }
    match ctx.opt_change(args)? {
        Some(id) => Ok(Root::Version(repo.current_version(id).await?.hash)),
        None => Ok(Root::trunk()),
    }
}
