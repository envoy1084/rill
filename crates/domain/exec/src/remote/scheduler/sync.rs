//! Buildbarn's worker protocol: a worker reports its state with every
//! `Synchronize` and is told what to do next.

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
use crate::remote::operation::Stage;
use crate::remote::platform::platform_key;
use crate::remote::timestamps::proto_timestamp as timestamp;

impl Scheduler {
    /// Answer one worker report.
    pub async fn synchronize(&self, request: SynchronizeRequest) -> Result<SynchronizeResponse> {
        let worker = worker_key(&request.worker_id);
        let session = self.session(&worker).await?;
        let state = request
            .current_state
            .and_then(|CurrentState { worker_state }| worker_state);
        tracing::debug!(
            worker,
            state = ?state.as_ref().map(state_name),
            session = ?session.as_ref().map(|s| s.action.short()),
            "worker report"
        );
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
                        Ok(()) => Ok(self.carry_on()),
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
                let action_bytes = if op.secrets.is_empty() {
                    action_bytes
                } else {
                    match self.with_secrets(&space, &op, action_bytes).await {
                        Ok((bytes, command)) => {
                            op.secret_command = Some(command);
                            bytes
                        }
                        Err(e) => {
                            let failed = failure(rill_reapi_proto::rpc::Status {
                                code: tonic::Code::FailedPrecondition as i32,
                                message: format!("giving the action its secrets: {e}"),
                                details: vec![],
                            });
                            self.finish(&space, &queue, &claim, &mut op, failed, None)
                                .await?;
                            continue;
                        }
                    }
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
    pub(super) async fn abandon(&self, worker: &str, session: &WorkerSession) -> Result<()> {
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

    fn executing(&self, session: &WorkerSession) -> SynchronizeResponse {
        let action = Action::decode(session.action_bytes.clone())
            .ok()
            .map(|a| self.bounded(a));
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

    /// No new desired state: the worker carries on with its action. (Any
    /// `Executing` state, even for the same action, restarts it.)
    fn carry_on(&self) -> SynchronizeResponse {
        SynchronizeResponse {
            next_synchronization_at: Some(timestamp(
                Timestamp::now().millis() + self.config.report_every.as_millis() as i64,
            )),
            desired_state: None,
        }
    }

    /// `action` with a timeout within the configured bounds; workers need
    /// one, and clients often leave it unset.
    fn bounded(&self, mut action: Action) -> Action {
        let asked = action
            .timeout
            .as_ref()
            .filter(|t| t.seconds > 0 || t.nanos > 0)
            .map(|t| std::time::Duration::new(t.seconds.max(0) as u64, t.nanos.max(0) as u32))
            .unwrap_or(self.config.default_timeout);
        let timeout = asked.min(self.config.max_timeout);
        action.timeout = Some(prost_types::Duration {
            seconds: timeout.as_secs() as i64,
            nanos: timeout.subsec_nanos() as i32,
        });
        action
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

fn state_name(state: &current_state::WorkerState) -> String {
    match state {
        current_state::WorkerState::Idle(()) => "idle".into(),
        current_state::WorkerState::Executing(e) => format!(
            "executing {} {}",
            e.action_digest.as_ref().map_or("?", |d| &d.hash[..12]),
            match &e.execution_state {
                Some(current_state::executing::ExecutionState::Completed(_)) => "completed",
                Some(_) => "progress",
                None => "unset",
            }
        ),
    }
}

/// An `ExecuteResponse` reporting that the action could not run.
pub fn failure(status: rill_reapi_proto::rpc::Status) -> ExecuteResponse {
    ExecuteResponse {
        status: Some(status),
        ..Default::default()
    }
}
