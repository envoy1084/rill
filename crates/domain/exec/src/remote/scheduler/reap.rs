//! Taking actions back from workers that stopped reporting.

use prost::Message;
use rill_reapi_proto::reapi::Platform;
use rill_store::KeyRange;

use super::handle::Scheduler;
use super::sync::failure;
use crate::error::Result;
use crate::remote::operation::Stage;
use crate::remote::platform::platform_key;

/// Dead jobs inspected per queue and sweep.
const DEAD_SCAN: usize = 100;

impl Scheduler {
    /// Put actions whose claims expired back in their queues, and fail the
    /// operations of actions that lost every attempt. Returns how many
    /// actions went back.
    pub async fn reap(&self) -> Result<usize> {
        let mut requeued = 0;
        for org in self.orgs_in_turn().await? {
            let space = self.space(&org);
            let platforms = space
                .records
                .meta()
                .scan(KeyRange::prefix(space.records.keys().exec_platforms()))
                .await?;
            for (_, value) in platforms {
                let platform = Platform::decode(value).unwrap_or_default();
                let queue = self.platform_queue(&space, &platform_key(Some(&platform)));
                requeued += queue.reap().await?;
                for dead in queue.dead(DEAD_SCAN).await? {
                    let id = String::from_utf8_lossy(&dead.job.payload).into_owned();
                    let Some(mut op) = self.operation(&space, &id).await? else {
                        continue;
                    };
                    if op.stage == Stage::Completed {
                        continue;
                    }
                    op.stage = Stage::Completed;
                    op.response = Some(
                        failure(rill_reapi_proto::rpc::Status {
                            code: tonic::Code::Unavailable as i32,
                            message: format!(
                                "the action lost its worker {} times",
                                self.config.max_attempts
                            ),
                            details: vec![],
                        })
                        .encode_to_vec()
                        .into(),
                    );
                    let keys = space.records.keys();
                    let tx = rill_store::Transaction::new()
                        .set(
                            keys.exec_operation(&id),
                            rill_store::repo::encode_record(&op)?,
                        )
                        .add(keys.exec_operation_count(&id), 1)
                        .delete(keys.exec_in_flight(&op.action));
                    space.records.meta().transact(tx).await?;
                }
            }
        }
        Ok(requeued)
    }
}
