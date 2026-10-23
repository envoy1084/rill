//! Daily digests: once a day, at the configured hour, everyone who asked
//! gets a summary of yesterday under each intent they own.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rill_policy::intents::day_of;
use rill_types::{IntentStatus, Timestamp};
use tokio_util::sync::CancellationToken;

use super::leader::while_leader;
use crate::error::Result;
use crate::notify::{notify_people, prefs, Message};
use crate::state::RepoContext;

const DAY_MS: i64 = 86_400_000;

/// Milliseconds from `now` to the next `hour`:00 UTC.
fn until_next(now: Timestamp, hour: u8) -> i64 {
    let ms = now.millis();
    let today = ms - ms.rem_euclid(DAY_MS) + i64::from(hour.min(23)) * 3_600_000;
    let next = if today > ms { today } else { today + DAY_MS };
    next - ms
}

/// Send the digests of the day before `now`, once.
pub async fn send(ctx: &RepoContext, now: Timestamp) -> Result<()> {
    let yesterday = day_of(Timestamp::from_millis(now.millis() - DAY_MS));
    let marker = format!("digest-sent:{yesterday}");
    match ctx
        .repo
        .set_misc_cas(&marker, None, Bytes::from_static(b"1"))
        .await
    {
        Ok(()) => {}
        Err(e) if e.is_conflict() => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let readers = prefs::digest_index(&ctx.repo).await?;
    if readers.is_empty() {
        return Ok(());
    }
    let review = ctx.review();
    let intents = ctx.repo.list_intents().await?;
    for reader in readers {
        let mut lines = Vec::new();
        for intent in intents.iter().filter(|i| i.owner.to_string() == reader) {
            if matches!(intent.status, IntentStatus::Abandoned) {
                continue;
            }
            let d = review.intent_digest(intent, &yesterday).await?;
            if d.landed == 0 && d.open == 0 {
                continue;
            }
            lines.push(format!(
                "{}: {} landed, {} reverted, {} flagged by reviewers, {} sampled for audit; {} in review",
                intent.title, d.landed, d.reverted, d.flagged, d.sampled, d.open
            ));
        }
        if lines.is_empty() {
            continue;
        }
        let m = Message::new(
            format!("{} on {yesterday}", ctx.config.name),
            lines.join("\n"),
        );
        notify_people(ctx, &[reader], None, &m).await?;
    }
    Ok(())
}

/// Send digests every day until `shutdown`, from one server.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    while_leader(ctx.repo.clone(), "digests", shutdown, |_lease, stop| {
        let ctx = ctx.clone();
        async move {
            loop {
                let hour = ctx.server_config.notify.digest_hour_utc;
                let wait = until_next(Timestamp::now(), hour);
                tokio::select! {
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(wait as u64)) => {}
                }
                if let Err(e) = send(&ctx, Timestamp::now()).await {
                    tracing::warn!(repo = %ctx.config.name, error = %e, "sending digests failed");
                }
            }
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_digest_is_today_or_tomorrow_at_the_hour() {
        let at = |h: i64, m: i64| Timestamp::from_millis(10 * DAY_MS + h * 3_600_000 + m * 60_000);
        assert_eq!(until_next(at(7, 0), 8), 3_600_000);
        assert_eq!(until_next(at(8, 0), 8), DAY_MS);
        assert_eq!(until_next(at(9, 30), 8), DAY_MS - 90 * 60_000);
    }
}
