//! Optimistic read-modify-write of reviews.

use rill_types::{ChangeId, Principal, ReviewState, Timestamp};

use crate::error::{Error, Result};
use crate::service::ReviewService;

/// How many times a write is retried after a concurrent update before the
/// operation gives up.
pub const MAX_WRITE_ATTEMPTS: usize = 8;

/// The actor for writes the review service makes on its own behalf.
pub(crate) fn service_actor() -> Principal {
    Principal::service("review")
}

impl ReviewService {
    /// Apply `mutate` to the current state of `change_id` and write back
    /// what changed, re-reading and re-applying on conflict. The mutator
    /// must be idempotent with respect to the state it is given; it returns
    /// a value that is handed back with the written state.
    pub(crate) async fn update<T>(
        &self,
        change_id: &ChangeId,
        actor: &Principal,
        mut mutate: impl FnMut(&mut ReviewState) -> Result<T>,
    ) -> Result<(ReviewState, T)> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let prev = self
                .repo
                .read_review(change_id)
                .await?
                .ok_or(Error::ReviewNotFound(*change_id))?;
            let mut next = prev.state.clone();
            let out = mutate(&mut next)?;
            if next == prev.state {
                return Ok((next, out));
            }
            // Only record changes move `updated_at`; votes and comments
            // leave the record alone so reviewers never conflict.
            if next.record() != prev.state.record() {
                next.updated_at = Timestamp::now().max(prev.state.updated_at);
            }
            match self.repo.write_review(&prev, &next, actor).await {
                Ok(()) => return Ok((next, out)),
                Err(e) if e.is_conflict() => {
                    tracing::debug!(change = %change_id, "review write conflict; retrying");
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Contended(*change_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::fixture;

    #[tokio::test(flavor = "multi_thread")]
    async fn update_retries_past_a_concurrent_record_write() {
        let fx = fixture().await;
        let (id, _) = fx.root_change("a.txt", "hello\n").await;
        fx.service.submit(&id, &fx.alice).await.unwrap();

        // A racing writer changes the record during the first attempt.
        let mut attempts = 0;
        let repo = fx.service.repo().clone();
        let (state, ()) = fx
            .service
            .update(&id, &fx.alice, |s| {
                attempts += 1;
                if attempts == 1 {
                    let repo = repo.clone();
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let prev = repo.read_review(&id).await.unwrap().unwrap();
                            let mut racing = prev.state.clone();
                            racing.extra.insert("racer".into(), "1".into());
                            repo.write_review(&prev, &racing, &fx.alice).await.unwrap();
                        })
                    });
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
