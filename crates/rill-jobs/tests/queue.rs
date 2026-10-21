//! Queue scenarios: ordering, retries, the dead-letter queue, expired
//! claims and fencing, and enqueueing inside a caller's transaction.

use std::time::Duration;

use bytes::Bytes;
use rill_jobs::{Error, JobSpec, Priority, Queue, RetryPolicy};
use rill_store::{RepoStore, Stores, Transaction};

async fn repo() -> RepoStore {
    let stores = Stores::memory();
    stores.repo(stores.create_repo("r", "").await.unwrap().id)
}

fn policy(lease_ms: u64, base_delay: Duration) -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay,
        max_delay: base_delay,
        lease: Duration::from_millis(lease_ms),
    }
}

#[tokio::test]
async fn urgent_jobs_first_then_in_the_order_they_came() {
    let queue = Queue::new(repo().await, "q");
    queue
        .push(JobSpec::new("t", "k", b"low".to_vec()).priority(Priority::Low))
        .await
        .unwrap();
    queue
        .push(JobSpec::new("t", "k", b"n1".to_vec()))
        .await
        .unwrap();
    queue
        .push(JobSpec::new("t", "k", b"n2".to_vec()))
        .await
        .unwrap();
    queue
        .push(JobSpec::new("t", "other", b"high".to_vec()).priority(Priority::High))
        .await
        .unwrap();

    let mut order = Vec::new();
    while let Some(claim) = queue.claim("w").await.unwrap() {
        order.push(String::from_utf8(claim.job.payload.clone()).unwrap());
        queue.complete(&claim).await.unwrap();
    }
    assert_eq!(order, ["high", "n1", "n2", "low"]);
    let stats = queue.stats().await.unwrap();
    assert_eq!(
        (stats.enqueued, stats.completed, stats.pending()),
        (4, 4, 0)
    );
}

#[tokio::test]
async fn a_failing_job_backs_off_retries_and_then_dies() {
    let repo = repo().await;
    let waiting =
        Queue::new(repo.clone(), "slow").with_policy(policy(1_000, Duration::from_secs(3600)));
    waiting.push(JobSpec::new("t", "k", vec![])).await.unwrap();
    let claim = waiting.claim("w").await.unwrap().unwrap();
    waiting.fail(&claim, "boom", false).await.unwrap();
    assert!(waiting.claim("w").await.unwrap().is_none(), "backing off");

    let queue = Queue::new(repo, "q").with_policy(policy(1_000, Duration::ZERO));
    queue.push(JobSpec::new("t", "k", vec![])).await.unwrap();
    for attempt in 0..3 {
        let claim = queue.claim("w").await.unwrap().expect("a retry");
        assert_eq!(claim.job.attempt, attempt);
        queue.fail(&claim, "boom", false).await.unwrap();
    }
    assert!(queue.claim("w").await.unwrap().is_none());
    let dead = queue.dead(10).await.unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].error, "boom");
    let stats = queue.stats().await.unwrap();
    assert_eq!((stats.retried, stats.dead, stats.pending()), (2, 1, 0));

    assert!(queue.retry_dead(&dead[0].job.id).await.unwrap());
    let claim = queue.claim("w").await.unwrap().expect("revived");
    assert_eq!(claim.job.attempt, 0);
    queue.complete(&claim).await.unwrap();
    let stats = queue.stats().await.unwrap();
    assert_eq!((stats.dead, stats.completed, stats.pending()), (0, 1, 0));

    queue.push(JobSpec::new("t", "k", vec![])).await.unwrap();
    let claim = queue.claim("w").await.unwrap().unwrap();
    queue.fail(&claim, "bad input", true).await.unwrap();
    assert_eq!(
        queue.dead(10).await.unwrap().len(),
        1,
        "a fatal failure dies at once"
    );
}

#[tokio::test]
async fn a_killed_workers_job_reruns_and_the_deposed_worker_is_fenced() {
    let repo = repo().await;
    let queue = Queue::new(repo.clone(), "q").with_policy(policy(50, Duration::ZERO));
    queue.push(JobSpec::new("t", "k", vec![])).await.unwrap();

    let stale = queue.claim("a").await.unwrap().unwrap();
    assert_eq!(queue.reap().await.unwrap(), 0, "the claim is live");
    // Worker `a` dies: it stops renewing.
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(queue.reap().await.unwrap(), 1);

    let fresh = queue.claim("b").await.unwrap().expect("the job reruns");
    assert_eq!(fresh.job.id, stale.job.id);
    assert_eq!(fresh.job.attempt, 1);
    assert_eq!(fresh.job.last_error.as_deref(), Some("claim expired"));

    assert!(matches!(queue.complete(&stale).await, Err(Error::Lost(_))));
    assert!(matches!(queue.renew(&stale).await, Err(Error::Lost(_))));
    let mut write = Transaction::new().set(repo.keys().misc("effect"), Bytes::new());
    write.conditions.push(stale.fence());
    assert!(repo.meta().transact(write).await.unwrap_err().is_conflict());

    let fresh = queue.renew(&fresh).await.unwrap();
    queue.complete(&fresh).await.unwrap();
    assert_eq!(queue.stats().await.unwrap().completed, 1);
}

#[tokio::test]
async fn a_job_is_enqueued_with_its_change_or_not_at_all() {
    let repo = repo().await;
    let queue = Queue::new(repo.clone(), "q");
    let key = repo.keys().misc("state");
    repo.meta()
        .set(key.clone(), Bytes::from_static(b"1"))
        .await
        .unwrap();

    let mut lost = Transaction::new()
        .expect_absent(key.clone())
        .set(key.clone(), Bytes::new());
    lost.mutations
        .extend(queue.enqueue(JobSpec::new("t", "k", vec![])).unwrap());
    assert!(repo.meta().transact(lost).await.unwrap_err().is_conflict());
    assert!(queue.claim("w").await.unwrap().is_none());

    let mut won = Transaction::new()
        .expect_present(key.clone())
        .set(key, Bytes::new());
    won.mutations
        .extend(queue.enqueue(JobSpec::new("t", "k", vec![])).unwrap());
    repo.meta().transact(won).await.unwrap();
    assert!(queue.claim("w").await.unwrap().is_some());
}

#[tokio::test]
async fn a_released_job_is_ready_again_without_losing_an_attempt() {
    let queue =
        Queue::new(repo().await, "q").with_policy(policy(60_000, Duration::from_secs(3600)));
    queue.push(JobSpec::new("t", "k", vec![])).await.unwrap();
    let first = queue.claim("a").await.unwrap().unwrap();
    queue.release(&first).await.unwrap();
    let again = queue.claim("b").await.unwrap().expect("ready at once");
    assert_eq!((again.job.id, again.job.attempt), (first.job.id, 0));
    assert!(matches!(queue.complete(&first).await, Err(Error::Lost(_))));
    queue.complete(&again).await.unwrap();
    assert_eq!(queue.stats().await.unwrap().pending(), 0);
}
