//! Handing a reviewed change to the lander and reporting its progress.

use rill_lander::LandRequest;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{RepoPathBuf, ReviewStatus};
use tonic::Request;

use super::reviews::change_id;
use super::ReviewSvc;
use crate::services::budget;
use crate::services::common::{self, open};
use rill_runtime::error::{Error, Result};

impl ReviewSvc {
    pub(super) async fn request_land_impl(
        &self,
        request: Request<pb::RequestLandRequest>,
    ) -> Result<pb::RequestLandResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().land().await?;
        let id = change_id(&request.get_ref().change_id)?;
        let review = opened.repo.review();
        let state = review.get(&id).await?;
        match state.status {
            ReviewStatus::Landed => {
                return Err(Error::precondition(format!("change {id} already landed")))
            }
            ReviewStatus::Landing => {
                return Err(Error::precondition(format!(
                    "change {id} is already landing"
                )))
            }
            ReviewStatus::Abandoned => {
                return Err(Error::precondition(format!("change {id} is abandoned")))
            }
            _ => {}
        }
        let decision = review.evaluate_for(&state, opened.principal()).await?;
        if !decision.allowed {
            return Err(Error::precondition(format!(
                "submit policy does not allow landing: {}",
                decision.reasons.join("; ")
            )));
        }
        let version = state
            .current_version()
            .ok_or_else(|| Error::precondition(format!("change {id} has no versions")))?;
        budget::spend_now(&opened, budget::LANDS).await?;
        let ticket = opened
            .repo
            .lander
            .enqueue(
                LandRequest::new(id, version, opened.principal().clone(), state.risk)
                    .sponsored_by(opened.ctx.claims.as_ref().and_then(|c| c.sponsor.clone())),
            )
            .await?;
        let (proto, _) = common::ticket_status(&opened.repo, ticket).await?;
        Ok(pb::RequestLandResponse {
            ticket: Some(proto),
        })
    }

    pub(super) async fn get_land_status_impl(
        &self,
        request: Request<pb::GetLandStatusRequest>,
    ) -> Result<pb::GetLandStatusResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ticket = common::parse_ticket(&request.get_ref().ticket)?;
        let (proto, status) = common::ticket_status(&opened.repo, ticket).await?;
        let (seq, landed_hash, error) = match status {
            rill_lander::LandStatus::Landed { seq } => {
                let hash = opened.repo.repo.trunk_at(seq).await?;
                (Some(seq.get()), hash.map(|h| hash_to_proto(&h)), None)
            }
            rill_lander::LandStatus::Failed { reason }
            | rill_lander::LandStatus::Returned { reason }
            | rill_lander::LandStatus::Ejected { reason } => (None, None, Some(reason)),
            rill_lander::LandStatus::Conflict { paths } => (
                None,
                None,
                Some(format!(
                    "rebase onto trunk conflicts at: {}",
                    paths
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ),
            _ => (None, None, None),
        };
        let place = if status_is_queued(&proto) {
            opened.repo.lander.position(ticket).await?
        } else {
            None
        };
        Ok(pb::GetLandStatusResponse {
            ticket: Some(proto),
            seq,
            landed_hash,
            error,
            position: place.map(|(at, _)| at as u32),
            eta_ms: place.map(|(_, wait)| wait.as_millis() as u64),
        })
    }
}

fn status_is_queued(ticket: &pb::LandTicket) -> bool {
    ticket.status == pb::LandStatus::Queued as i32
}
