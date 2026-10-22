//! A worker inside the server process. It speaks the same protocol as a
//! Buildbarn worker, through the scheduler directly instead of over gRPC,
//! so actions queue, merge, and recover from a lost worker the same way
//! whichever kind runs them.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use rill_reapi_proto::reapi::{platform::Property, Platform};
use rill_reapi_proto::remoteworker::current_state::executing::ExecutionState;
use rill_reapi_proto::remoteworker::{
    current_state, desired_state, CurrentState, SynchronizeRequest,
};

use super::execute::execute;
use crate::remote::Scheduler;
use crate::sandbox::Sandbox;

/// Pause after a failed report before trying again.
const RETRY_AFTER: Duration = Duration::from_secs(1);

/// A worker running actions with a local sandbox.
pub struct LocalWorker {
    scheduler: Arc<Scheduler>,
    sandbox: Arc<dyn Sandbox>,
    name: String,
    platform: Platform,
}

impl LocalWorker {
    /// A worker called `name` taking actions for `platform`.
    pub fn new(
        scheduler: Arc<Scheduler>,
        sandbox: Arc<dyn Sandbox>,
        name: impl Into<String>,
        platform: &BTreeMap<String, String>,
    ) -> Self {
        LocalWorker {
            scheduler,
            sandbox,
            name: name.into(),
            platform: Platform {
                properties: platform
                    .iter()
                    .map(|(name, value)| Property {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            },
        }
    }

    /// Take and run actions until `stop` completes. An action running
    /// when it does is abandoned; its claim lapses and another worker
    /// takes it.
    pub async fn run(self, stop: impl Future<Output = ()>) {
        tokio::pin!(stop);
        let mut state = idle();
        loop {
            let report = self.scheduler.synchronize(self.request(state.clone()));
            let response = tokio::select! {
                _ = &mut stop => break,
                response = report => response,
            };
            let response = match response {
                Ok(response) => response,
                Err(e) => {
                    tracing::warn!(worker = %self.name, error = %e, "local worker report failed");
                    state = idle();
                    tokio::select! {
                        _ = &mut stop => break,
                        _ = tokio::time::sleep(RETRY_AFTER) => continue,
                    }
                }
            };
            state = match response.desired_state.and_then(|d| d.worker_state) {
                Some(desired_state::WorkerState::Executing(task)) => {
                    match self.execute(task, &mut stop).await {
                        Some(state) => state,
                        None => break,
                    }
                }
                Some(desired_state::WorkerState::Idle(())) => idle(),
                None => state,
            };
        }
        tracing::info!(worker = %self.name, "local worker stopped");
    }

    /// Run one action, reporting progress so the claim stays ours. Returns
    /// the state to report next, or `None` when stopped.
    async fn execute(
        &self,
        task: desired_state::Executing,
        stop: &mut std::pin::Pin<&mut impl Future<Output = ()>>,
    ) -> Option<CurrentState> {
        let digest = task.action_digest.clone();
        let Some(action) = task.action else {
            return Some(idle());
        };
        let space = self.scheduler.space(&task.instance_name_suffix);
        let run = execute(
            &self.scheduler,
            &space,
            self.sandbox.as_ref(),
            &self.name,
            &action,
            task.queued_timestamp,
        );
        tokio::pin!(run);
        let every = self.scheduler.config().report_every;
        loop {
            tokio::select! {
                _ = &mut *stop => return None,
                response = &mut run => {
                    return Some(executing(digest, ExecutionState::Completed(response)));
                }
                _ = tokio::time::sleep(every) => {
                    let progress = executing(digest.clone(), ExecutionState::Running(()));
                    match self.scheduler.synchronize(self.request(progress)).await {
                        Ok(r) if r.desired_state.is_some() => {
                            tracing::warn!(worker = %self.name, "local worker lost its action");
                            return Some(idle());
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(worker = %self.name, error = %e, "progress report failed");
                        }
                    }
                }
            }
        }
    }

    fn request(&self, state: CurrentState) -> SynchronizeRequest {
        SynchronizeRequest {
            worker_id: HashMap::from([("worker".to_string(), self.name.clone())]),
            platform: Some(self.platform.clone()),
            current_state: Some(state),
            ..Default::default()
        }
    }
}

fn idle() -> CurrentState {
    CurrentState {
        worker_state: Some(current_state::WorkerState::Idle(())),
    }
}

fn executing(
    action_digest: Option<rill_reapi_proto::reapi::Digest>,
    state: ExecutionState,
) -> CurrentState {
    CurrentState {
        worker_state: Some(current_state::WorkerState::Executing(
            current_state::Executing {
                action_digest,
                execution_state: Some(state),
            },
        )),
    }
}
