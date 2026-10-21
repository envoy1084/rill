//! A resumable tail of a repository's event log.
//!
//! The tail reads the event counter before each page. When the page comes
//! back empty it waits for the counter to move past that reading, so an
//! event committed between the read and the wait still wakes it.

use std::collections::VecDeque;
use std::time::Duration;

use futures::Stream;
use rill_store::RepoStore;
use rill_types::{Event, Versionstamp};

use crate::error::Result;
use crate::filter::EventFilter;

/// Events read per store scan.
const PAGE: usize = 256;
/// How long one wait for new events lasts before the tail looks again.
const WAIT: Duration = Duration::from_secs(5);

/// Where a tail starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// After the newest event: only new events.
    Now,
    /// From the oldest retained event.
    Beginning,
    /// After this cursor, from an earlier event.
    After(Versionstamp),
}

struct State {
    repo: RepoStore,
    cursor: Option<Versionstamp>,
    filter: EventFilter,
    buffer: VecDeque<(Versionstamp, Event)>,
}

/// The events of `repo` passing `filter`, with their cursors, oldest
/// first and then as they are committed. The stream never ends on its
/// own. A cursor older than the retention window resumes at the oldest
/// retained event.
pub async fn tail(
    repo: RepoStore,
    start: Start,
    filter: EventFilter,
) -> Result<impl Stream<Item = Result<(Versionstamp, Event)>> + Send> {
    let cursor = match start {
        Start::Now => repo.latest_event().await?,
        Start::Beginning => None,
        Start::After(stamp) => Some(stamp),
    };
    let state = State {
        repo,
        cursor,
        filter,
        buffer: VecDeque::new(),
    };
    Ok(futures::stream::try_unfold(state, |mut s| async move {
        loop {
            if let Some(item) = s.buffer.pop_front() {
                return Ok(Some((item, s)));
            }
            let seen = s.repo.event_count().await?;
            let page = s.repo.events_after(s.cursor, PAGE).await?;
            let Some((last, _)) = page.last() else {
                s.repo.wait_for_events(seen, WAIT).await?;
                continue;
            };
            s.cursor = Some(*last);
            s.buffer
                .extend(page.into_iter().filter(|(_, e)| s.filter.matches(e)));
        }
    }))
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use rill_store::Stores;
    use rill_types::Principal;

    use super::*;

    async fn grant(repo: &RepoStore, subject: &str) {
        repo.add_relation("repo", "reader", subject, &Principal::human("a"))
            .await
            .unwrap();
    }

    fn subject(event: &Event) -> String {
        match &event.kind {
            rill_types::EventKind::RelationChanged { subject, .. } => subject.clone(),
            other => other.name().to_string(),
        }
    }

    #[tokio::test]
    async fn replays_then_follows_and_resumes_after_a_cursor() {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        grant(&repo, "a").await;

        let filter = EventFilter::kinds(["relation_changed"]);
        let mut all = Box::pin(
            tail(repo.clone(), Start::Beginning, filter.clone())
                .await
                .unwrap(),
        );
        let mut new = Box::pin(
            tail(repo.clone(), Start::Now, filter.clone())
                .await
                .unwrap(),
        );
        let (first, event) = all.next().await.unwrap().unwrap();
        assert_eq!(subject(&event), "a");

        let writer = {
            let repo = repo.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                grant(&repo, "b").await;
                grant(&repo, "c").await;
            })
        };
        for expected in ["b", "c"] {
            assert_eq!(subject(&all.next().await.unwrap().unwrap().1), expected);
            assert_eq!(subject(&new.next().await.unwrap().unwrap().1), expected);
        }
        writer.await.unwrap();

        let mut resumed = Box::pin(tail(repo, Start::After(first), filter).await.unwrap());
        assert_eq!(subject(&resumed.next().await.unwrap().unwrap().1), "b");
    }
}
