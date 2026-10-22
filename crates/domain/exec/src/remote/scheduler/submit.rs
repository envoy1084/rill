//! Queueing an action, or joining the operation already running it.

use bytes::Bytes;
use prost::Message;
use rill_jobs::{JobSpec, Priority};
use rill_reapi_proto::reapi::Platform;
use rill_store::repo::{decode_record, encode_record};
use rill_store::{Error as StoreError, Transaction};
use rill_types::ids::random_id;
use rill_types::Timestamp;

use super::handle::Scheduler;
use crate::error::{Error, Result};
use crate::remote::digest::BlobDigest;
use crate::remote::operation::{OperationRecord, Stage};
use crate::remote::platform::platform_key;
use crate::remote::space::ExecSpace;

/// Attempts at queueing against concurrent submitters of the same action.
const ATTEMPTS: usize = 8;

/// The job kind of a queued action.
pub const EXECUTE_JOB: &str = "execute";

/// An action to run.
#[derive(Debug, Clone)]
pub struct Submission {
    pub action: BlobDigest,
    pub platform: Option<Platform>,
    pub priority: Priority,
    pub requester: String,
    pub do_not_cache: bool,
}

/// What submitting did.
#[derive(Debug, Clone)]
pub struct Submitted {
    pub operation: OperationRecord,
    /// The action was already running; this request joined it.
    pub merged: bool,
}

/// REAPI priorities: lower is more urgent, 0 is the default.
pub fn priority_of(reapi_priority: i32) -> Priority {
    match reapi_priority {
        p if p < 0 => Priority::High,
        0 => Priority::Normal,
        _ => Priority::Low,
    }
}

impl Scheduler {
    /// Queue `submission`, unless the same action is already queued or
    /// running, in which case the caller shares that operation.
    pub async fn submit(&self, space: &ExecSpace, submission: Submission) -> Result<Submitted> {
        let keys = space.records.keys();
        let in_flight = keys.exec_in_flight(&submission.action.hash);
        let platform = platform_key(submission.platform.as_ref());
        let queue = self.platform_queue(space, &platform);
        for _ in 0..ATTEMPTS {
            if let Some(id) = space.records.meta().get(&in_flight).await? {
                let id = String::from_utf8_lossy(&id).into_owned();
                if let Some(op) = self.operation(space, &id).await? {
                    if op.stage != Stage::Completed {
                        return Ok(Submitted {
                            operation: op,
                            merged: true,
                        });
                    }
                }
            }
            let op = OperationRecord {
                id: hex::encode(random_id()),
                org: space.org.clone(),
                action: submission.action.hash,
                action_size: submission.action.size,
                stage: Stage::Queued,
                requester: submission.requester.clone(),
                do_not_cache: submission.do_not_cache,
                queued_at: Timestamp::now(),
                worker: None,
                response: None,
            };
            let job = JobSpec::new(
                EXECUTE_JOB,
                submission.action.hash.to_hex(),
                op.id.clone().into_bytes(),
            )
            .priority(submission.priority);
            let current = space.records.meta().get(&in_flight).await?;
            let mut tx = Transaction::new()
                .expect(in_flight.clone(), current)
                .set(in_flight.clone(), Bytes::from(op.id.clone()))
                .set(keys.exec_operation(&op.id), encode_record(&op)?)
                .add(keys.exec_operation_count(&op.id), 1)
                .set(
                    keys.exec_platform(&platform),
                    Bytes::from(
                        submission
                            .platform
                            .clone()
                            .unwrap_or_default()
                            .encode_to_vec(),
                    ),
                )
                .add(self.instance.keys().exec_wake(), 1);
            tx.mutations.extend(queue.enqueue(job)?);
            match space.records.meta().transact(tx).await {
                Ok(_) => {
                    return Ok(Submitted {
                        operation: op,
                        merged: false,
                    })
                }
                Err(StoreError::Conflict(_)) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Contended("queueing the action".into()))
    }

    /// The operation `id` of `space`.
    pub async fn operation(&self, space: &ExecSpace, id: &str) -> Result<Option<OperationRecord>> {
        let key = space.records.keys().exec_operation(id);
        match space.records.meta().get(&key).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The update count of operation `id`, to pass to
    /// [`Scheduler::wait_operation`].
    pub async fn operation_updates(&self, space: &ExecSpace, id: &str) -> Result<i64> {
        let key = space.records.keys().exec_operation_count(id);
        Ok(space.records.meta().counter(&key).await?)
    }

    /// Wait until operation `id` changed after `seen` updates, or `timeout`.
    pub async fn wait_operation(
        &self,
        space: &ExecSpace,
        id: &str,
        seen: i64,
        timeout: std::time::Duration,
    ) -> Result<()> {
        let key = space.records.keys().exec_operation_count(id);
        space
            .records
            .meta()
            .wait_counter(&key, seen, timeout)
            .await?;
        Ok(())
    }
}
