//! Resolving the change a review command addresses.

use rill_daemon::proto as dpb;
use rill_daemon::ChangeKind;
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

/// Refuse to submit or land the checkout's own working change while the
/// checkout holds something the change does not: unresolved conflicts, a
/// recorded version that still conflicts, or a base the change was not
/// recorded on (after `rill sync`). Landing the
/// recorded version then would re-create conflicts the checkout already
/// resolved.
pub async fn ensure_recorded(ctx: &mut Ctx<'_>, repo: &RepoClient, id: ChangeId) -> Result<()> {
    if ctx.working_change().await? != Some(id) {
        return Ok(());
    }
    let mount = ctx.mount().await?;
    let base = mount.base_version.clone();
    let reply = ctx
        .daemon()
        .await?
        .get_status(dpb::GetStatusRequest {
            name: mount.name,
            prefixes: Vec::new(),
        })
        .await?;
    let conflicted: Vec<String> = rill_daemon::client::local_changes(&reply.changes)
        .into_iter()
        .filter(|c| c.kind == ChangeKind::Conflict)
        .map(|c| c.path.into_string())
        .collect();
    if !conflicted.is_empty() {
        return Err(CliError::conflict(format!(
            "{} still conflicted in the checkout ({}); resolve with `rill resolve`, then `rill amend`",
            conflicted.len(),
            conflicted.join(", ")
        )));
    }
    let info = repo.current_version(id).await?;
    if info.has_conflicts {
        return Err(CliError::conflict(format!(
            "{id}'s recorded version has conflicts (left by a failed landing or rebase); \
             record the checkout's resolution with `rill amend`"
        )));
    }
    let parent = info.parents.first().map(|p| p.to_hex()).unwrap_or_default();
    if parent != base {
        return Err(CliError::conflict(format!(
            "{id} is not recorded on the checkout's base {} (the checkout moved, e.g. by `rill sync`); \
             record it there with `rill amend`",
            &base[..base.len().min(12)]
        )));
    }
    Ok(())
}
