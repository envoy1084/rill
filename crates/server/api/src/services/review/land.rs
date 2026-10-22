//! Handing a reviewed change to the lander and reporting its progress.

use rill_lander::LandRequest;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use std::sync::Arc;

use rill_lander::Lander;
use rill_types::{
    ChangeId, ChangeVersionHash, GroupMode, Line, RefName, RepoPathBuf, ReviewState, ReviewStatus,
    RiskClass,
};
use tonic::Request;

use super::reviews::change_id;
use super::ReviewSvc;
use crate::services::budget;
use crate::services::common::{self, open, Opened};
use rill_runtime::error::{Error, Result};

impl ReviewSvc {
    pub(super) async fn request_land_impl(
        &self,
        request: Request<pb::RequestLandRequest>,
    ) -> Result<pb::RequestLandResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().land().await?;
        let id = change_id(&request.get_ref().change_id)?;
        let state = opened.repo.review().get(&id).await?;
        let version = landable_version(&opened, &state).await?;
        let (line, lander) = line_and_lander(&opened, request.get_ref().onto.as_deref()).await?;
        budget::spend_now(&opened, budget::LANDS).await?;
        let ticket = lander
            .enqueue(land_request(&opened, id, version, state.risk).onto(line))
            .await?;
        let (proto, _) = common::ticket_status(&opened.repo, ticket).await?;
        Ok(pb::RequestLandResponse {
            ticket: Some(proto),
        })
    }

    pub(super) async fn request_land_stack_impl(
        &self,
        request: Request<pb::RequestLandStackRequest>,
    ) -> Result<pb::RequestLandStackResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().land().await?;
        let id = change_id(&request.get_ref().change_id)?;
        let (line, lander) = line_and_lander(&opened, request.get_ref().onto.as_deref()).await?;
        let review = opened.repo.review();
        let stack = review.stack_of(&id).await?;
        let top = stack.iter().position(|c| *c == id).unwrap_or(0);
        let mut states = Vec::with_capacity(top + 1);
        for member in &stack[..=top] {
            let state = review.get(member).await?;
            if state.status != ReviewStatus::Landed {
                states.push(state);
            }
        }
        if states.is_empty() {
            return Err(Error::precondition(format!(
                "every change in the stack up to {id} already landed"
            )));
        }
        let mut requests = Vec::with_capacity(states.len());
        let mut below: Option<ChangeVersionHash> = None;
        for state in &states {
            let version = landable_version(&opened, state).await?;
            if let Some(below) = below {
                let cv = opened.repo.derive.change(&version).await?;
                if cv.first_parent() != Some(below) {
                    return Err(Error::precondition(format!(
                        "change {} is not based on the current version of the change \
                         below it; sync the stack first",
                        state.change_id
                    )));
                }
            }
            below = Some(version);
            requests.push(
                land_request(&opened, state.change_id, version, state.risk).onto(line.clone()),
            );
        }
        for _ in &requests {
            budget::spend_now(&opened, budget::LANDS).await?;
        }
        let mode = match request.get_ref().prefix {
            true => GroupMode::Prefix,
            false => GroupMode::All,
        };
        let tickets = lander.enqueue_group(requests, mode).await?;
        let mut out = Vec::with_capacity(tickets.len());
        for ticket in tickets {
            out.push(common::ticket_status(&opened.repo, ticket).await?.0);
        }
        Ok(pb::RequestLandStackResponse { tickets: out })
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
                let line = opened
                    .repo
                    .lander
                    .queue()
                    .record(ticket)
                    .await?
                    .map(|r| r.request.line)
                    .unwrap_or_default();
                let hash = opened.repo.repo.line_at(&line, seq).await?;
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

/// The line `onto` names ("trunk" when absent) and its lander. A release
/// must have been cut.
async fn line_and_lander(opened: &Opened, onto: Option<&str>) -> Result<(Line, Arc<Lander>)> {
    let line = match onto {
        None | Some("") => Line::Trunk,
        Some(name) => RefName::new(name)
            .ok()
            .and_then(|r| Line::of_ref(&r))
            .ok_or_else(|| {
                Error::invalid(format!(
                    "onto is \"trunk\" or \"release/<name>\", not {name:?}"
                ))
            })?,
    };
    match &line {
        Line::Trunk => Ok((line, opened.repo.lander.clone())),
        Line::Release(name) => {
            if opened.repo.repo.line_head(&line).await?.is_none() {
                return Err(Error::not_found(format!("{line} was never cut")));
            }
            let lander = opened.repo.releases.get(name);
            Ok((line, lander))
        }
    }
}

/// The version of `state` that may be asked to land by the caller now.
async fn landable_version(opened: &Opened, state: &ReviewState) -> Result<ChangeVersionHash> {
    let id = state.change_id;
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
    let decision = opened
        .repo
        .review()
        .evaluate_for(state, opened.principal())
        .await?;
    if !decision.allowed {
        return Err(Error::precondition(format!(
            "submit policy does not allow landing {id}: {}",
            decision.reasons.join("; ")
        )));
    }
    state
        .current_version()
        .ok_or_else(|| Error::precondition(format!("change {id} has no versions")))
}

fn land_request(
    opened: &Opened,
    id: ChangeId,
    version: ChangeVersionHash,
    risk: RiskClass,
) -> LandRequest {
    LandRequest::new(id, version, opened.principal().clone(), risk)
        .sponsored_by(opened.ctx.claims.as_ref().and_then(|c| c.sponsor.clone()))
}

fn status_is_queued(ticket: &pb::LandTicket) -> bool {
    ticket.status == pb::LandStatus::Queued as i32
}
