//! Reverting landings: one at a time, or every landing one origin made.

use rill_lander::healing::Selector;
use rill_proto::convert::{id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{Timestamp, TrunkSeq};

use super::TrunkSvc;
use crate::services::common::{self, open_repo, Opened};
use crate::services::{names, operations, Call};
use rill_runtime::error::{Error, Result};

impl TrunkSvc {
    pub(super) async fn revert_landing_impl(
        &self,
        request: Call<pb::RevertLandingRequest>,
    ) -> Result<pb::RevertLandingResponse> {
        let (opened, seq) = self.landing(&request, &request.get_ref().landing).await?;
        opened.permit().land().await?;
        let reason = match request.get_ref().reason.as_str() {
            "" => format!("revert of {seq}"),
            given => given.to_string(),
        };
        let outcome = rill_lander::revert(
            &opened.repo.lander,
            seq,
            &reason,
            opened.principal().clone(),
        )
        .await?;
        let landing = common::landing(&opened.repo, outcome.seq, outcome.hash).await?;
        Ok(pb::RevertLandingResponse {
            landing: landing.into(),
            version: pb::ChangeVersion::from(outcome.version).into(),
        })
    }

    pub(super) async fn preview_bulk_revert_impl(
        &self,
        request: Call<pb::PreviewBulkRevertRequest>,
    ) -> Result<pb::PreviewBulkRevertResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().admin().await?;
        let selector = selector(request.get_ref().selector.as_option())?;
        let entries = opened
            .repo
            .lander
            .plan_reverts(&selector)
            .await?
            .into_iter()
            .map(|e| pb::BulkRevertEntry {
                landing: names::trunk_position(&opened.repo, e.seq),
                position: e.seq.get(),
                change_id: id_to_proto(&e.change),
                summary: e.summary,
                conflicts: e.conflicts.iter().map(ToString::to_string).collect(),
                land_request: String::new(),
            })
            .collect();
        Ok(pb::PreviewBulkRevertResponse { entries })
    }

    pub(super) async fn bulk_revert_impl(
        &self,
        request: Call<pb::BulkRevertRequest>,
    ) -> Result<pb::BulkRevertResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().admin().await?;
        let req = request.get_ref();
        let selector = selector(req.selector.as_option())?;
        let reason = match req.reason.is_empty() {
            true => format!("mass revert of {}", selector.describe()),
            false => req.reason.clone(),
        };
        let id = names::request_id(&req.request_id)?;
        let mut operation = operations::started(&opened, &id, "bulk_revert");
        let queued = opened
            .repo
            .lander
            .apply_reverts(&selector, &reason, opened.principal())
            .await;
        operation.result = Some(match queued {
            Ok(queued) => pb::operation::Result::BulkRevert(pb::BulkRevertResult {
                entries: queued
                    .into_iter()
                    .map(|(seq, ticket)| entry(&opened, seq, &ticket.0.to_string()))
                    .collect(),
            }),
            Err(e) => pb::operation::Result::Error(operations::status_of(e.into())),
        });
        operation.done = true;
        operation.updated_at = timestamp_to_proto(Timestamp::now());
        operations::save(&opened.repo, &id, &operation).await?;
        Ok(pb::BulkRevertResponse {
            operation: operation.into(),
        })
    }
}

fn entry(opened: &Opened, seq: TrunkSeq, ticket: &str) -> pb::BulkRevertEntry {
    pb::BulkRevertEntry {
        landing: names::trunk_position(&opened.repo, seq),
        position: seq.get(),
        land_request: names::land_request(&opened.repo, ticket),
        ..Default::default()
    }
}

fn selector(given: Option<&pb::RevertSelector>) -> Result<Selector> {
    let given = given.ok_or(rill_proto::ConvertError::missing("selector"))?;
    let selector = Selector {
        session: given.session.clone(),
        model: given.model.clone(),
        sponsor: given.sponsor.clone(),
        author: given.author.clone(),
        intent: match &given.intent {
            Some(name) => names::intent(name)?,
            None => None,
        },
        since: given.since.map(TrunkSeq),
        until: given.until.map(TrunkSeq),
    };
    if !selector.names_origin() {
        return Err(Error::invalid(
            "name a session, model, sponsor, author or intent to revert",
        ));
    }
    Ok(selector)
}
