//! Clients watching the event log, as agents do; each event's delivery
//! latency (commit to receipt) is recorded as `watch_event`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use rill_sdk::{EventQuery, RepoClient, WatchStart};
use rill_types::Timestamp;

use crate::stats::Stats;

/// Watch the log with `count` streams until `deadline`.
pub async fn watch(repo: RepoClient, count: usize, deadline: Instant, stats: Arc<Stats>) {
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..count {
        let (repo, stats) = (repo.clone(), stats.clone());
        tasks.spawn(async move {
            while Instant::now() < deadline {
                let Ok(mut stream) = repo
                    .watch_events(WatchStart::Now, EventQuery::default())
                    .await
                else {
                    stats.error("watch_event");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                let until = tokio::time::Instant::from_std(deadline);
                loop {
                    let next = tokio::select! {
                        next = stream.next() => next,
                        _ = tokio::time::sleep_until(until) => return,
                    };
                    match next {
                        Some(Ok(entry)) => {
                            let lag = Timestamp::now().millis() - entry.event.at.millis();
                            stats.ok("watch_event", Duration::from_millis(lag.max(0) as u64));
                        }
                        // A server went away; reconnect elsewhere.
                        Some(Err(_)) | None => break,
                    }
                }
            }
        });
    }
    while tasks.join_next().await.is_some() {}
}
