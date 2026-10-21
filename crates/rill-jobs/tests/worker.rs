//! The worker runtime end to end.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use rill_jobs::{Claim, Handler, Job, JobFailure, JobSpec, Queue, RetryPolicy, Worker};
use rill_store::Stores;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Recorder {
    runs: Mutex<HashMap<String, u32>>,
}

#[async_trait]
impl Handler for Recorder {
    async fn run(&self, job: &Job, _claim: &Claim) -> Result<(), JobFailure> {
        let runs = {
            let mut runs = self.runs.lock();
            let n = runs.entry(job.key.clone()).or_default();
            *n += 1;
            *n
        };
        match job.kind.as_str() {
            "flaky" if runs == 1 => Err(JobFailure::Retry("first try fails".into())),
            "slow" => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn workers_retry_failures_and_keep_long_jobs_claimed() {
    let stores = Stores::memory();
    let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
    let queue = Queue::new(repo, "q").with_policy(RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
        lease: Duration::from_millis(90),
    });
    let handler = Arc::new(Recorder::default());
    let shutdown = CancellationToken::new();
    let worker = Worker::new(queue.clone(), handler.clone()).concurrency(2);
    let running = tokio::spawn(worker.run(shutdown.clone()));

    queue
        .push(JobSpec::new("flaky", "a", vec![]))
        .await
        .unwrap();
    queue.push(JobSpec::new("slow", "b", vec![])).await.unwrap();
    queue
        .push(JobSpec::new("plain", "c", vec![]))
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while queue.stats().await.unwrap().completed < 3 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "jobs did not finish"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    shutdown.cancel();
    running.await.unwrap();

    let runs = handler.runs.lock().clone();
    assert_eq!(runs["a"], 2, "the flaky job ran again");
    assert_eq!(
        runs["b"], 1,
        "renewal kept the slow job from being taken back"
    );
    assert_eq!(runs["c"], 1);
    let stats = queue.stats().await.unwrap();
    assert_eq!((stats.retried, stats.dead, stats.pending()), (1, 0, 0));
}
