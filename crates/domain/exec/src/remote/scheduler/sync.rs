//! Buildbarn's worker protocol: a worker reports its state with every
//! `Synchronize` and is told what to do next.

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::{digest_function, Action, ExecuteResponse};
use rill_reapi_proto::remoteworker::{
    current_state, desired_state, CurrentState, DesiredState, SynchronizeRequest,
    SynchronizeResponse,
};
use rill_store::repo::{decode_record, encode_record};
use rill_store::{Error as StoreError, Transaction};
use rill_types::Timestamp;

use super::handle::Scheduler;
use super::session::{worker_key, WorkerSession};
use crate::error::{Error, Result};
use crate::remote::digest::BlobDigest;
use crate::remote::operation::{OperationRecord, Stage};
use crate::remote::platform::platform_key;

impl Scheduler {
    /// Answer one worker report.
    pub async fn synchronize(&self, request: SynchronizeRequest) -> Result<SynchronizeResponse> {
        let worker = worker_key(&request.worker_id);
        let session = self.session(&worker).await?;
        let state = request
            .current_state
            .and_then(|CurrentState { worker_state }| worker_state);
        match (state, session) {
            (Some(current_state::WorkerState::Executing(executing)), Some(session)) => {
                let reported = executing
                    .action_digest
                    .as_ref()
                    .map(BlobDigest::parse)
                    .transpose()?;
                if reported.map(|d| d.hash) != Some(session.action) {
                    self.abandon(&worker, &session).await?;
                } else if let Some(current_state::executing::ExecutionState::Completed(response)) =
                    executing.execution_state
                {
                    self.complete(&worker, &session, response).await?;
                } else {
                    return match self.keep(&worker, &session).await {
                        Ok(()) => Ok(self.executing(&session)),
                        Err(Error::Jobs(rill_jobs::Error::Lost(_))) => {
                            Ok(self.idle(Timestamp::now()))
                        }
                        Err(e) => Err(e),
                    };
                }
            }
            (_, Some(session)) => self.abandon(&worker, &session).await?,
            _ => {}
        }
        if request.prefer_being_idle {
            return Ok(self.idle(Timestamp::now()));
        }
        let platform = platform_key(request.platform.as_ref());
        let seen = self
            .instance
            .meta()
            .counter(&self.instance.keys().exec_wake())
            .await?;
        if let Some(session) = self.assign(&worker, &platform).await? {
            return Ok(self.executing(&session));
        }
        self.instance
            .meta()
            .wait_counter(
                &self.instance.keys().exec_wake(),
                seen,
                self.config.idle_wait,
            )
            .await?;
        match self.assign(&worker, &platform).await? {
            Some(session) => Ok(self.executing(&session)),
            None => Ok(self.idle(Timestamp::now())),
        }
    }

    pub(super) async fn session(&self, worker: &str) -> Result<Option<WorkerSession>> {
        let key = self.instance.keys().exec_worker(worker);
        match self.instance.meta().get(&key).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Claim the next action for `worker` on `platform`, serving orgs in
    /// turn.
    async fn assign(&self, worker: &str, platform: &str) -> Result<Option<WorkerSession>> {
        for org in self.orgs_in_turn().await? {
            let space = self.space(&org);
            let queue = self.platform_queue(&space, platform);
            while let Some(claim) = queue.claim(worker).await? {
                let id = String::from_utf8_lossy(&claim.job.payload).into_owned();
                let Some(mut op) = self.operation(&space, &id).await? else {
                    queue.complete(&claim).await?;
                    continue;
                };
                if op.stage == Stage::Completed {
                    queue.complete(&claim).await?;
                    continue;
                }
                let digest = op.action_digest();
                let Some(action_bytes) = space.read(&digest).await? else {
                    let failed = failure(rill_reapi_proto::rpc::Status {
                        code: tonic::Code::FailedPrecondition as i32,
                        message: format!("action {digest} is not in the CAS"),
                        details: vec![],
                    });
                    self.finish(&space, &queue, &claim, &mut op, failed, None)
                        .await?;
                    continue;
                };
                op.stage = Stage::Executing;
                op.worker = Some(worker.to_string());
                let session = WorkerSession {
                    org: org.clone(),
                    queue: queue.name().to_string(),
                    claim: claim.clone(),
                    operation: id.clone(),
                    action: op.action,
                    action_size: op.action_size,
                    action_bytes,
                    queued_at: op.queued_at,
                };
                let keys = space.records.keys();
                let mut tx = Transaction::new()
                    .set(keys.exec_operation(&id), encode_record(&op)?)
                    .add(keys.exec_operation_count(&id), 1)
                    .set(
                        self.instance.keys().exec_worker(worker),
                        encode_record(&session)?,
                    );
                tx.conditions.push(claim.fence());
                match space.records.meta().transact(tx).await {
                    Ok(_) => return Ok(Some(session)),
                    Err(StoreError::Conflict(_)) => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(None)
    }

    /// Renew `session`'s claim on a progress report.
    async fn keep(&self, worker: &str, session: &WorkerSession) -> Result<()> {
        let space = self.space(&session.org);
        let queue = self.queue(&space, &session.queue);
        let claim = queue.renew(&session.claim).await?;
        let renewed = WorkerSession {
            claim,
            ..session.clone()
        };
        self.instance
            .meta()
            .set(
                self.instance.keys().exec_worker(worker),
                encode_record(&renewed)?,
            )
            .await?;
        Ok(())
    }

    /// The worker dropped its action: hand it to another.
    async fn abandon(&self, worker: &str, session: &WorkerSession) -> Result<()> {
        let space = self.space(&session.org);
        let queue = self.queue(&space, &session.queue);
        match queue.release(&session.claim).await {
            Ok(()) | Err(rill_jobs::Error::Lost(_)) => {}
            Err(e) => return Err(e.into()),
        }
        self.instance
            .meta()
            .transact(Transaction::new().delete(self.instance.keys().exec_worker(worker)))
            .await?;
        Ok(())
    }

    /// Record a finished action: its operation, the cache, the queue.
    async fn complete(
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
        let cacheable = !op.do_not_cache
            && response.status.as_ref().is_none_or(|s| s.code == 0)
            && response.result.as_ref().is_some_and(|r| r.exit_code == 0)
            && !Action::decode(session.action_bytes.clone()).is_ok_and(|a| a.do_not_cache);
        if cacheable {
            if let Some(result) = &response.result {
                space.put_result(&digest, result).await?;
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

    /// Complete `op` with `response` and finish its job, in one commit.
    pub(super) async fn finish(
        &self,
        space: &crate::remote::space::ExecSpace,
        queue: &rill_jobs::Queue,
        claim: &rill_jobs::Claim,
        op: &mut OperationRecord,
        response: ExecuteResponse,
        worker: Option<&str>,
    ) -> Result<()> {
        op.stage = Stage::Completed;
        op.response = Some(Bytes::from(response.encode_to_vec()));
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
        match space.records.meta().transact(tx).await {
            Ok(_) => Ok(()),
            Err(StoreError::Conflict(_)) => Err(Error::Jobs(rill_jobs::Error::Lost(claim.job.id))),
            Err(e) => Err(e.into()),
        }
    }

    fn executing(&self, session: &WorkerSession) -> SynchronizeResponse {
        let action = Action::decode(session.action_bytes.clone()).ok();
        SynchronizeResponse {
            next_synchronization_at: Some(timestamp(
                Timestamp::now().millis() + self.config.report_every.as_millis() as i64,
            )),
            desired_state: Some(DesiredState {
                worker_state: Some(desired_state::WorkerState::Executing(
                    desired_state::Executing {
                        action_digest: Some(
                            BlobDigest {
                                hash: session.action,
                                size: session.action_size,
                            }
                            .to_proto(),
                        ),
                        action,
                        queued_timestamp: Some(timestamp(session.queued_at.millis())),
                        auxiliary_metadata: vec![],
                        instance_name_suffix: session.org.clone(),
                        w3c_trace_context: Default::default(),
                        digest_function: digest_function::Value::Blake3 as i32,
                    },
                )),
            }),
        }
    }

    fn idle(&self, now: Timestamp) -> SynchronizeResponse {
        SynchronizeResponse {
            next_synchronization_at: Some(timestamp(now.millis())),
            desired_state: Some(DesiredState {
                worker_state: Some(desired_state::WorkerState::Idle(())),
            }),
        }
    }
}

/// An `ExecuteResponse` reporting that the action could not run.
pub fn failure(status: rill_reapi_proto::rpc::Status) -> ExecuteResponse {
    ExecuteResponse {
        status: Some(status),
        ..Default::default()
    }
}

fn timestamp(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis.div_euclid(1000),
        nanos: (millis.rem_euclid(1000) * 1_000_000) as i32,
    }
}
