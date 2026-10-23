//! Who may cast an `Owner` approval: someone who counts for an owner set
//! (from trunk's `OWNERS.toml` files) the change touches, or, for touched
//! paths no file covers, an `owner` of them by relation. The submit policy
//! decides whether every set has the approvals it requires.

use rill_policy::owners::{counts_for, Approver};
use rill_policy::{Relation, Verb};
use rill_types::{ChangeId, Timestamp};

use crate::services::common::Opened;
use rill_runtime::error::{Error, Result};
use rill_runtime::writes::write_set;

pub use rill_policy::submit::OWNER_LABEL;

/// Refuse an `Owner` approval from someone who owns nothing the change's
/// current version writes.
pub async fn check_approval(opened: &Opened, change: &ChangeId) -> Result<()> {
    if opened.dev_mode() {
        return Ok(());
    }
    let ctx = &opened.repo;
    let review = ctx.review();
    let state = review.get(change).await?;
    let version = state
        .current_version()
        .ok_or_else(|| Error::precondition(format!("{change} has no version")))?;
    let voter = &opened.ctx.principal;
    let me = Approver {
        principal: voter.clone(),
        teams: opened.ctx.teams.clone(),
    };
    let owners = review.trunk_owners().await?;
    let cv = ctx.derive.change(&version).await?;
    let set = write_set(&ctx.trees, &ctx.derive, &cv).await?;
    let now = Timestamp::now();
    let mut counts = false;
    let mut uncovered = Vec::new();
    for path in &set.writes {
        let sets = owners.sets_for(path);
        if sets.is_empty() {
            uncovered.push(path);
        } else if sets.iter().any(|s| counts_for(s, &me, now)) {
            counts = true;
        }
    }
    if counts {
        return Ok(());
    }
    let mut owns_uncovered = !uncovered.is_empty();
    for path in uncovered {
        if opened
            .permit()
            .check(Verb::Review, Relation::Owner, path)
            .await
            .is_err()
        {
            owns_uncovered = false;
            break;
        }
    }
    if owns_uncovered {
        return Ok(());
    }
    Err(Error::denied(format!(
        "{voter} is in no owner set this change touches, so cannot approve as {OWNER_LABEL}"
    )))
}
