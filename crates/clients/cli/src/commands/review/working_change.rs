//! Resolving the change a review command addresses.

use rill_sdk::RepoClient;
use rill_types::ChangeId;

use crate::commands::change_ref;
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// Resolve a change argument, defaulting to the checkout's working change.
pub async fn change_or_working(
    ctx: &mut Ctx<'_>,
    change: Option<&str>,
    repo: &RepoClient,
) -> Result<ChangeId> {
    match change {
        Some(c) => change_ref(c)?.id(repo).await,
        None => ctx
            .working_change()
            .await?
            .ok_or_else(|| CliError::usage("no working change in this checkout; pass a change id")),
    }
}
