//! Optimistic read-modify-write of review records.

use rill_types::{ChangeId, ReviewState, Timestamp};

use crate::error::{Error, Result};
use crate::service::ReviewService;

/// How many times a write is retried after a concurrent update before the
/// operation gives up.
pub const MAX_WRITE_ATTEMPTS: usize = 8;

impl ReviewService {
    /// Replace `prev` with `next` if the stored record still equals `prev`.
    /// Returns `Ok(false)` when another writer got there first.
    pub(crate) async fn try_put(&self, next: &ReviewState, prev: &ReviewState) -> Result<bool> {
        match self.repo.put_review(next, Some(prev)).await {
            Ok(()) => Ok(true),
            Err(e) if e.is_conflict() => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Apply `mutate` to the current state of `change_id` and write it back,
    /// re-reading and re-applying on conflict. The mutator must be
    /// idempotent with respect to the state it is given; it returns a value
    /// that is handed back with the written state.
    pub(crate) async fn update<T>(
        &self,
        change_id: &ChangeId,
        mut mutate: impl FnMut(&mut ReviewState) -> Result<T>,
    ) -> Result<(ReviewState, T)> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let prev = self.get(change_id).await?;
            let mut next = prev.clone();
            let out = mutate(&mut next)?;
            if next == prev {
                return Ok((next, out));
            }
            next.updated_at = Timestamp::now().max(prev.updated_at);
            if self.try_put(&next, &prev).await? {
                return Ok((next, out));
            }
            tracing::debug!(change = %change_id, "review write conflict; retrying");
        }
        Err(Error::Contended(*change_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::fixture;

    #[tokio::test(flavor = "multi_thread")]
    async fn update_retries_past_a_concurrent_write() {
        let fx = fixture().await;
        let (id, _) = fx.root_change("a.txt", "hello\n").await;
        fx.service.submit(&id, &fx.alice).await.unwrap();

        // Simulate a racing writer on the first attempt only.
        let mut attempts = 0;
        let repo = fx.service.repo().clone();
        let (state, ()) = fx
            .service
            .update(&id, |s| {
                attempts += 1;
                if attempts == 1 {
                    let mut racing = s.clone();
                    racing.extra.insert("racer".into(), "1".into());
                    // The store is in-memory, so a blocking write here is
                    // only visible to the test; the real write races below.
                    let repo = repo.clone();
                    let prev = s.clone();
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(repo.put_review(&racing, Some(&prev)))
                    })
                    .unwrap();
                }
                s.extra.insert("mine".into(), "1".into());
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(state.extra.get("racer").map(String::as_str), Some("1"));
        assert_eq!(state.extra.get("mine").map(String::as_str), Some("1"));
    }
}
