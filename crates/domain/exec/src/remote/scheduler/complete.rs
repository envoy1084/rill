//! A worker finishing an action: recording its result, caching it, and
//! sometimes running it again to check it is hermetic.

use bytes::Bytes;
use prost::Message;
use rill_jobs::{JobSpec, Priority};
use rill_reapi_proto::reapi::{Action, ExecuteResponse};
use rill_store::repo::encode_record;
use rill_store::{Error as StoreError, Transaction};
use rill_types::ids::random_id;
use rill_types::Timestamp;

use super::handle::Scheduler;
use super::session::WorkerSession;
use super::submit::EXECUTE_JOB;
use crate::error::{Error, Result};
use crate::remote::cache::{differences, probe::sampled};
use crate::remote::operation::{OperationRecord, Stage};
use crate::remote::space::ExecSpace;

impl Scheduler {
    /// Record a finished action: its operation, the cache, the queue.
    pub(super) async fn complete(
        &self,
        worker: &str,
        session: &WorkerSession,
        response: ExecuteResponse,
    ) -> Result<()> {
        let space = self.space(&session.org);
        let queue = self.queue(&space, &session.queue);
        let Some(mut op) = self.operation(&space, &session.operation).await? else {
            return self.abandon(worker, session).await;
        };
        let digest = op.action_digest();
        let mut response = response;
        if !op.secrets.is_empty() {
            self.redact_response(&space, &op, &mut response).await?;
        }
        let succeeded = response.status.as_ref().is_none_or(|s| s.code == 0)
            && response.result.as_ref().is_some_and(|r| r.exit_code == 0);
        if op.probe {
            self.judge_probe(&space, &op, &response).await?;
        } else if succeeded && self.cacheable(&space, &op, session).await? {
            if let Some(result) = &response.result {
                let cached = self.cache_result(&space, &digest, result).await?;
                if cached && sampled(self.config.probe_rate) {
                    self.probe(&space, &session.queue, &op).await?;
                }
            }
        }
        match self
            .finish(
                &space,
                &queue,
                &session.claim,
                &mut op,
                response,
                Some(worker),
            )
            .await
        {
            Ok(()) | Err(Error::Jobs(rill_jobs::Error::Lost(_))) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Whether `op`'s result may enter the cache: not marked
    /// `do_not_cache`, carrying no secrets, and not of a label flagged
    /// non-hermetic.
    async fn cacheable(
        &self,
        space: &ExecSpace,
        op: &OperationRecord,
        session: &WorkerSession,
    ) -> Result<bool> {
        if !op.secrets.is_empty()
            || op.do_not_cache
            || Action::decode(session.action_bytes.clone()).is_ok_and(|a| a.do_not_cache)
        {
            return Ok(false);
        }
        match &op.label {
            Some(label) => Ok(self.nonhermetic(space, label).await?.is_none()),
            None => Ok(true),
        }
    }

    /// Queue `op`'s action again, as a probe, at low priority.
    async fn probe(&self, space: &ExecSpace, queue_name: &str, op: &OperationRecord) -> Result<()> {
        let probe = OperationRecord {
            id: hex::encode(random_id()),
            stage: Stage::Queued,
            requester: "probe".into(),
            do_not_cache: true,
            queued_at: Timestamp::now(),
            worker: None,
            response: None,
            probe: true,
            ..op.clone()
        };
        let queue = self.queue(space, queue_name);
        let job = JobSpec::new(
            EXECUTE_JOB,
            probe.action.to_hex(),
            probe.id.clone().into_bytes(),
        )
        .priority(Priority::Low);
        let keys = space.records.keys();
        let mut tx = Transaction::new()
            .set(keys.exec_operation(&probe.id), encode_record(&probe)?)
            .add(keys.exec_operation_count(&probe.id), 1)
            .add(self.instance.keys().exec_wake(), 1);
        tx.mutations.extend(queue.enqueue(job)?);
        space.records.meta().transact(tx).await?;
        tracing::debug!(action = %op.action.short(), label = ?op.label, "hermeticity probe queued");
        Ok(())
    }

    /// Compare a probe's result with the cached one; a difference evicts
    /// the cached result and flags the label.
    async fn judge_probe(
        &self,
        space: &ExecSpace,
        op: &OperationRecord,
        response: &ExecuteResponse,
    ) -> Result<()> {
        let digest = op.action_digest();
        let (Some(rerun), Some(cached)) = (
            response.result.as_ref(),
            self.cached_result(space, &digest).await?,
        ) else {
            return Ok(());
        };
        let differ = differences(&cached, rerun);
        if differ.is_empty() {
            tracing::debug!(action = %digest.hash.short(), "hermeticity probe matched");
            return Ok(());
        }
        tracing::warn!(action = %digest.hash.short(), label = ?op.label, ?differ, "an action is not hermetic; its result is no longer cached");
        self.evict_result(space, &digest).await?;
        if let Some(label) = &op.label {
            self.flag_nonhermetic(space, label, op.action, differ)
                .await?;
        }
        Ok(())
    }

    /// Complete `op` with `response` and finish its job, in one commit.
    pub(super) async fn finish(
        &self,
        space: &ExecSpace,
        queue: &rill_jobs::Queue,
        claim: &rill_jobs::Claim,
        op: &mut OperationRecord,
        response: ExecuteResponse,
        worker: Option<&str>,
    ) -> Result<()> {
        op.stage = Stage::Completed;
        op.response = Some(Bytes::from(response.encode_to_vec()));
        let secret_command = op.secret_command.take();
        let keys = space.records.keys();
        let in_flight = keys.exec_in_flight(&op.action);
        let current = space.records.meta().get(&in_flight).await?;
        let mut tx = queue
            .complete_tx(claim)
            .set(keys.exec_operation(&op.id), encode_record(op)?)
            .add(keys.exec_operation_count(&op.id), 1);
        if current.as_deref() == Some(op.id.as_bytes()) {
            tx = tx.expect(in_flight.clone(), current).delete(in_flight);
        }
        if let Some(worker) = worker {
            tx = tx.delete(self.instance.keys().exec_worker(worker));
        }
        if let Some(command) = secret_command {
            tx = tx.delete(keys.exec_secret_command(&command));
        }
        match space.records.meta().transact(tx).await {
            Ok(_) => Ok(()),
            Err(StoreError::Conflict(_)) => Err(Error::Jobs(rill_jobs::Error::Lost(claim.job.id))),
            Err(e) => Err(e.into()),
        }
    }
}
