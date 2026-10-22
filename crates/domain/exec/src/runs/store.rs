//! Verification runs of a repository, written as they progress and
//! expired after the retention period.

use std::time::Duration;

use bytes::Bytes;
use rill_store::repo::{decode_record, encode_record};
use rill_store::{KeyRange, RepoStore, Transaction};
use rill_types::ids::random_id;
use rill_types::{ObjectHash, Timestamp};

use super::record::{IndexedStep, Run, RunEnd, RunHeader, StepRecord};
use crate::error::Result;

/// Expired runs removed each time a run starts.
const PRUNE_BATCH: usize = 32;

/// A repository's verification runs.
#[derive(Clone)]
pub struct RunStore {
    repo: RepoStore,
    retention: Duration,
}

/// A run being written.
#[derive(Clone)]
pub struct RunHandle {
    store: RunStore,
    pub id: String,
}

impl RunStore {
    /// Runs in `repo`, kept for `retention`.
    pub fn new(repo: RepoStore, retention: Duration) -> Self {
        RunStore { repo, retention }
    }

    /// Start a run verifying `version`, removing runs past retention.
    pub async fn start(&self, version: ObjectHash, trunk: bool) -> Result<RunHandle> {
        self.prune().await?;
        let started_at = Timestamp::now();
        // Ids sort newest first, so listing is a forward scan.
        let id = format!(
            "{:016x}{}",
            i64::MAX - started_at.millis(),
            hex::encode(&random_id()[..4])
        );
        let header = RunHeader {
            id: id.clone(),
            version,
            trunk,
            started_at,
            end: None,
        };
        let keys = self.repo.keys();
        let tx = Transaction::new()
            .set(keys.verify_run(&id), encode_record(&header)?)
            .set(
                keys.verify_run_of_version(&version, &id),
                Bytes::from(id.clone()),
            )
            .add(keys.verify_run_count(&id), 1);
        self.repo.meta().transact(tx).await?;
        Ok(RunHandle {
            store: self.clone(),
            id,
        })
    }

    /// The run `id` with its steps.
    pub async fn get(&self, id: &str) -> Result<Option<Run>> {
        let keys = self.repo.keys();
        let Some(bytes) = self.repo.meta().get(&keys.verify_run(id)).await? else {
            return Ok(None);
        };
        let header: RunHeader = decode_record(&bytes)?;
        let steps = self
            .repo
            .meta()
            .scan(KeyRange::prefix(keys.verify_run_steps(id)))
            .await?
            .into_iter()
            .map(|(_, v)| decode_record(&v))
            .collect::<std::result::Result<_, _>>()?;
        Ok(Some(Run { header, steps }))
    }

    /// The latest `limit` runs, or those of `version`, newest first,
    /// without their steps.
    pub async fn list(&self, version: Option<&ObjectHash>, limit: usize) -> Result<Vec<RunHeader>> {
        let keys = self.repo.keys();
        let ids: Vec<String> = match version {
            None => {
                let rows = self
                    .repo
                    .meta()
                    .scan(KeyRange::prefix(keys.verify_runs()).limit(limit))
                    .await?;
                return rows
                    .into_iter()
                    .map(|(_, v)| Ok(decode_record(&v)?))
                    .collect();
            }
            Some(version) => self
                .repo
                .meta()
                .scan(KeyRange::prefix(keys.verify_runs_of_version(version)).limit(limit))
                .await?
                .into_iter()
                .map(|(_, id)| String::from_utf8_lossy(&id).into_owned())
                .collect(),
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(bytes) = self.repo.meta().get(&keys.verify_run(&id)).await? {
                out.push(decode_record(&bytes)?);
            }
        }
        out.sort_by(|a: &RunHeader, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// The update count of run `id`, for [`RunStore::wait`].
    pub async fn updates(&self, id: &str) -> Result<i64> {
        Ok(self
            .repo
            .meta()
            .counter(&self.repo.keys().verify_run_count(id))
            .await?)
    }

    /// Wait until run `id` changes after `seen` updates, or `timeout`.
    pub async fn wait(&self, id: &str, seen: i64, timeout: Duration) -> Result<()> {
        self.repo
            .meta()
            .wait_counter(&self.repo.keys().verify_run_count(id), seen, timeout)
            .await?;
        Ok(())
    }

    /// Delete the oldest runs started before the retention period.
    async fn prune(&self) -> Result<()> {
        let keys = self.repo.keys();
        let cutoff = Timestamp::now().millis() - self.retention.as_millis() as i64;
        let oldest = self
            .repo
            .meta()
            .scan(
                KeyRange::prefix(keys.verify_runs())
                    .reverse()
                    .limit(PRUNE_BATCH),
            )
            .await?;
        let mut tx = Transaction::new();
        let mut pruned = 0;
        for (key, value) in oldest {
            let header: RunHeader = decode_record(&value)?;
            if header.started_at.millis() >= cutoff {
                break;
            }
            let steps = self
                .repo
                .meta()
                .scan(KeyRange::prefix(keys.verify_run_steps(&header.id)))
                .await?;
            tx = tx
                .delete(key)
                .delete(keys.verify_run_of_version(&header.version, &header.id))
                .delete(keys.verify_run_count(&header.id));
            for (step, _) in steps {
                tx = tx.delete(step);
            }
            pruned += 1;
        }
        if pruned > 0 {
            self.repo.meta().transact(tx).await?;
            tracing::debug!(pruned, "verification runs past retention removed");
        }
        Ok(())
    }
}

impl RunHandle {
    /// Write step `index`.
    pub async fn step(&self, index: u32, record: &StepRecord) -> Result<()> {
        let keys = self.store.repo.keys();
        let tx = Transaction::new()
            .set(
                keys.verify_run_step(&self.id, index),
                encode_record(&IndexedStep {
                    index,
                    step: record.clone(),
                })?,
            )
            .add(keys.verify_run_count(&self.id), 1);
        self.store.repo.meta().transact(tx).await?;
        Ok(())
    }

    /// Mark the run finished.
    pub async fn finish(&self, green: bool, summary: String) -> Result<()> {
        let keys = self.store.repo.keys();
        let key = keys.verify_run(&self.id);
        let Some(bytes) = self.store.repo.meta().get(&key).await? else {
            return Ok(());
        };
        let mut header: RunHeader = decode_record(&bytes)?;
        header.end = Some(RunEnd {
            green,
            summary,
            finished_at: Timestamp::now(),
        });
        let tx = Transaction::new()
            .set(key, encode_record(&header)?)
            .add(keys.verify_run_count(&self.id), 1);
        self.store.repo.meta().transact(tx).await?;
        Ok(())
    }
}
