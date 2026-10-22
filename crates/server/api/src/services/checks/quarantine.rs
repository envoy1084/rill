//! Quarantined targets: listing them, and quarantining or lifting one.

use rill_exec::history::{Quarantine, TestHistory};
use rill_proto::convert::{id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;

use super::service::ChecksSvc;
use crate::listing::Page;
use crate::services::common::open_repo;
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

fn quarantine_proto(ctx: &RepoContext, q: Quarantine) -> pb::Quarantine {
    pb::Quarantine {
        name: names::quarantine(ctx, &q.target),
        target: q.target,
        since: timestamp_to_proto(q.since),
        by: pb::Principal::from(q.by).into(),
        reason: q.reason,
        intent: q.intent.map(|i| id_to_proto(&i)),
    }
}

impl ChecksSvc {
    pub(super) async fn list_quarantines_impl(
        &self,
        request: Call<pb::ListQuarantinesRequest>,
    ) -> Result<pb::ListQuarantinesResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let targets = TestHistory::new(opened.repo.repo.clone())
            .quarantined()
            .await?;
        let (targets, next_page_token) = page.of(targets)?;
        Ok(pb::ListQuarantinesResponse {
            quarantines: targets
                .into_iter()
                .map(|q| quarantine_proto(&opened.repo, q))
                .collect(),
            next_page_token,
        })
    }

    pub(super) async fn create_quarantine_impl(
        &self,
        request: Call<pb::CreateQuarantineRequest>,
    ) -> Result<pb::CreateQuarantineResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().admin().await?;
        let given = request
            .get_ref()
            .quarantine
            .as_option()
            .ok_or(rill_proto::ConvertError::missing("quarantine"))?;
        check_target(&given.target)?;
        let quarantine = Quarantine {
            target: given.target.clone(),
            since: Timestamp::now(),
            by: opened.principal().clone(),
            reason: given.reason.clone(),
            intent: None,
        };
        let added = TestHistory::new(opened.repo.repo.clone())
            .quarantine(&quarantine)
            .await?;
        if !added {
            return Err(Error::AlreadyExists(format!(
                "{} is already quarantined",
                given.target
            )));
        }
        Ok(pb::CreateQuarantineResponse {
            quarantine: quarantine_proto(&opened.repo, quarantine).into(),
        })
    }

    pub(super) async fn delete_quarantine_impl(
        &self,
        request: Call<pb::DeleteQuarantineRequest>,
    ) -> Result<pb::DeleteQuarantineResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().admin().await?;
        let target = names::quarantine_in(&opened.repo, name)?;
        let released = TestHistory::new(opened.repo.repo.clone())
            .release(&target, opened.principal())
            .await?;
        if !released {
            return Err(Error::not_found(format!("{target} is not quarantined")));
        }
        Ok(pb::DeleteQuarantineResponse {})
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
