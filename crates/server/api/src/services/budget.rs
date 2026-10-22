//! Spending registered agents' daily budgets.

use rill_store::meta::Transaction;
use rill_store::repo::day_of;
use rill_types::Timestamp;

use crate::services::common::Opened;
use rill_runtime::error::{Error, Result};

/// Versions stored per day.
pub const VERSIONS: &str = "versions";
/// Land requests per day.
pub const LANDS: &str = "lands";

/// Attempts at spending against concurrent spenders.
const ATTEMPTS: usize = 8;

/// The writes spending one unit of the caller's `metric` budget, for the
/// transaction doing the work; `None` when the caller has no such budget.
/// Refused once the budget is spent.
pub async fn spend(opened: &Opened, metric: &str) -> Result<Option<Transaction>> {
    let Some(scope) = &opened.ctx.scope else {
        return Ok(None);
    };
    let limit = match metric {
        VERSIONS => scope.budgets.versions_per_day,
        LANDS => scope.budgets.lands_per_day,
        _ => None,
    };
    let Some(limit) = limit else {
        return Ok(None);
    };
    let day = day_of(Timestamp::now());
    let principal = opened.principal();
    match opened
        .repo
        .repo
        .stores()
        .identity()
        .spend(principal, metric, day, limit)
        .await?
    {
        Some(tx) => Ok(Some(tx)),
        None => Err(Error::Exhausted(format!(
            "agent {principal} has spent its {limit} {metric} for today"
        ))),
    }
}

/// Spend one unit of the caller's `metric` budget on its own.
pub async fn spend_now(opened: &Opened, metric: &str) -> Result<()> {
    for _ in 0..ATTEMPTS {
        let Some(tx) = spend(opened, metric).await? else {
            return Ok(());
        };
        match opened.repo.repo.meta().transact(tx).await {
            Ok(_) => return Ok(()),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Unavailable("budget contended; retry".into()))
}
