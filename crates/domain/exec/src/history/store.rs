//! The test history of a repository: recent runs per target and decayed
//! flake statistics.

use rill_store::repo::{decode_record, encode_record};
use rill_store::{KeyRange, RepoStore, Transaction};
use rill_types::ids::random_id;
use rill_types::Timestamp;

use super::outcome::TestRun;
use super::score::FlakeStats;
use crate::error::Result;

/// Runs kept per target; older ones are deleted as new ones arrive.
pub const RUNS_KEPT: usize = 100;

/// A target and how flaky it is.
#[derive(Debug, Clone, PartialEq)]
pub struct FlakyTarget {
    pub target: String,
    pub score: f64,
    pub stats: FlakeStats,
}

/// A target's statistics as stored, with its name.
#[derive(serde::Serialize, serde::Deserialize)]
struct StatsRecord {
    target: String,
    stats: FlakeStats,
}

impl rill_types::Record for StatsRecord {
    const TAG: &'static str = "test-stats";
}

/// A repository's test history.
#[derive(Clone)]
pub struct TestHistory {
    repo: RepoStore,
}

impl TestHistory {
    pub fn new(repo: RepoStore) -> Self {
        TestHistory { repo }
    }

    /// Record `run`: compare it with the last run on the same action,
    /// update the target's statistics, and drop runs past
    /// [`RUNS_KEPT`].
    pub async fn record(&self, run: &TestRun) -> Result<()> {
        let keys = self.repo.keys();
        let recent = self.runs(&run.target, RUNS_KEPT).await?;
        let previous = run.action.and_then(|action| {
            recent
                .iter()
                .find(|r| r.action == Some(action) && r.outcome.is_verdict())
                .map(|r| r.outcome)
        });
        let stats_key = keys.test_stats(&run.target);
        let mut stats = match self.repo.meta().get(&stats_key).await? {
            Some(bytes) => decode_record::<StatsRecord>(&bytes)?.stats,
            None => FlakeStats::default(),
        };
        stats.observe(run.outcome, run.retried, previous, run.at.millis());
        let mut tx = Transaction::new()
            .set(
                keys.test_run(&run.target, run.at.millis(), &random_id()),
                encode_record(run)?,
            )
            .set(
                stats_key,
                encode_record(&StatsRecord {
                    target: run.target.clone(),
                    stats,
                })?,
            );
        if recent.len() >= RUNS_KEPT {
            let rows = self
                .repo
                .meta()
                .scan(KeyRange::prefix(keys.test_runs(&run.target)))
                .await?;
            for (key, _) in rows.into_iter().skip(RUNS_KEPT - 1) {
                tx = tx.delete(key);
            }
        }
        self.repo.meta().transact(tx).await?;
        Ok(())
    }

    /// The latest `limit` runs of `target`, newest first.
    pub async fn runs(&self, target: &str, limit: usize) -> Result<Vec<TestRun>> {
        let rows = self
            .repo
            .meta()
            .scan(KeyRange::prefix(self.repo.keys().test_runs(target)).limit(limit))
            .await?;
        rows.into_iter()
            .map(|(_, value)| Ok(decode_record(&value)?))
            .collect()
    }

    /// Targets scoring at least `min_score`, flakiest first, at most
    /// `limit`.
    pub async fn flaky(&self, min_score: f64, limit: usize) -> Result<Vec<FlakyTarget>> {
        let rows = self
            .repo
            .meta()
            .scan(KeyRange::prefix(self.repo.keys().all_test_stats()))
            .await?;
        let now = Timestamp::now().millis();
        let mut out = Vec::new();
        for (_, value) in rows {
            let record: StatsRecord = decode_record(&value)?;
            let score = record.stats.score(now);
            if score >= min_score {
                out.push(FlakyTarget {
                    target: record.target,
                    score,
                    stats: record.stats,
                });
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(limit);
        Ok(out)
    }

    /// The statistics of `target`.
    pub async fn stats(&self, target: &str) -> Result<Option<FlakeStats>> {
        match self
            .repo
            .meta()
            .get(&self.repo.keys().test_stats(target))
            .await?
        {
            Some(bytes) => Ok(Some(decode_record::<StatsRecord>(&bytes)?.stats)),
            None => Ok(None),
        }
    }
}
