//! Quarantined targets: listing them, and quarantining or lifting one.

use rill_exec::history::{Quarantine, TestHistory};
use rill_proto::convert::{id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use tonic::Request;

use super::service::ChecksSvc;
use crate::services::common::open;
use rill_runtime::error::{Error, Result};

impl ChecksSvc {
    pub(super) async fn quarantined(
        &self,
        request: Request<pb::ListQuarantinedRequest>,
    ) -> Result<pb::ListQuarantinedResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let targets = TestHistory::new(opened.repo.repo.clone())
            .quarantined()
            .await?;
        Ok(pb::ListQuarantinedResponse {
            targets: targets
                .into_iter()
                .map(|q| pb::QuarantinedTarget {
                    target: q.target,
                    since: timestamp_to_proto(q.since),
                    by: Some(q.by.into()),
                    reason: q.reason,
                    intent: q.intent.map(|i| id_to_proto(&i)),
                })
                .collect(),
        })
    }

    pub(super) async fn quarantine_impl(
        &self,
        request: Request<pb::QuarantineRequest>,
    ) -> Result<pb::QuarantineResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().admin().await?;
        let req = request.get_ref();
        check_target(&req.target)?;
        let added = TestHistory::new(opened.repo.repo.clone())
            .quarantine(&Quarantine {
                target: req.target.clone(),
                since: Timestamp::now(),
                by: opened.principal().clone(),
                reason: req.reason.clone(),
                intent: None,
            })
            .await?;
        Ok(pb::QuarantineResponse { added })
    }

    pub(super) async fn release_impl(
        &self,
        request: Request<pb::ReleaseQuarantineRequest>,
    ) -> Result<pb::ReleaseQuarantineResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().admin().await?;
        let released = TestHistory::new(opened.repo.repo.clone())
            .release(&request.get_ref().target, opened.principal())
            .await?;
        Ok(pb::ReleaseQuarantineResponse { released })
    }
}

/// A target names a step (`step:`) or a test case (`test:`).
fn check_target(target: &str) -> Result<()> {
    let named = ["step:", "test:"]
        .iter()
        .any(|p| target.strip_prefix(p).is_some_and(|rest| !rest.is_empty()));
    match named {
        true => Ok(()),
        false => Err(Error::invalid(format!(
            "a target is `step:<name>` or `test:<id>`, not {target:?}"
        ))),
    }
}
