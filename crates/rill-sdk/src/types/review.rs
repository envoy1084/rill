//! Review, land-ticket and review-event records.

use rill_proto::convert::{
    hash_from_proto, id_from_proto, opt_hash_from_proto, required, review_status_from_proto,
    risk_from_proto, timestamp_from_proto, trunk_seq_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::objects::review::CommentCategory;
use rill_types::{
    ChangeId, ChangeVersionHash, IntentId, Label, LabelVote, Principal, PrincipalId, ReviewState,
    ReviewStatus, RiskClass, Thread, ThreadAnchor, Timestamp, TrunkSeq,
};

use crate::error::{Error, Result};

/// A review state with the repository's label definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewInfo {
    /// The review state.
    pub review: ReviewState,
    /// Labels that can be voted on.
    pub labels: Vec<Label>,
}

/// Filters for listing reviews.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewFilter {
    /// Only these statuses; empty means any.
    pub status: Vec<ReviewStatus>,
    /// Only reviews owned by this principal.
    pub owner: Option<PrincipalId>,
    /// Only reviews linked to this intent.
    pub intent: Option<IntentId>,
    /// Page size; `0` lets the server choose.
    pub limit: u32,
    /// Continue from a previous page.
    pub page_token: Option<String>,
}

/// A new comment opening a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    /// Where the thread is anchored.
    pub anchor: ThreadAnchor,
    /// Comment text.
    pub body: String,
    /// Comment category.
    pub category: Option<CommentCategory>,
    /// Reviewer confidence, 0-100.
    pub confidence: Option<u8>,
    /// Who the thread is directed at.
    pub to: Option<Principal>,
}

impl NewComment {
    /// A change-level comment.
    pub fn on_change(body: impl Into<String>) -> Self {
        NewComment {
            anchor: ThreadAnchor::Change,
            body: body.into(),
            category: None,
            confidence: None,
            to: None,
        }
    }
}

/// A reply on an existing thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Comment text.
    pub body: String,
    /// Comment category.
    pub category: Option<CommentCategory>,
    /// Reviewer confidence, 0-100.
    pub confidence: Option<u8>,
}

impl Reply {
    /// A plain reply.
    pub fn new(body: impl Into<String>) -> Self {
        Reply {
            body: body.into(),
            category: None,
            confidence: None,
        }
    }
}

/// A new thread with its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThread {
    /// Thread id.
    pub thread_id: String,
    /// The thread.
    pub thread: Thread,
}

/// Outcome of evaluating submit rules for a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitDecision {
    /// True when the change may land.
    pub allowed: bool,
    /// Human-readable reasons for the decision.
    pub reasons: Vec<String>,
    /// Policies that fired.
    pub matched_policies: Vec<String>,
    /// Risk classification of the current version.
    pub risk: RiskClass,
    /// Why the risk is what it is.
    pub risk_reasons: Vec<String>,
}

impl TryFrom<pb::SubmitDecision> for SubmitDecision {
    type Error = Error;

    fn try_from(d: pb::SubmitDecision) -> Result<Self> {
        Ok(SubmitDecision {
            allowed: d.allowed,
            reasons: d.reasons,
            matched_policies: d.matched_policies,
            risk: risk_from_proto("submit_decision.risk", d.risk)?,
            risk_reasons: d.risk_reasons,
        })
    }
}

/// Where a landing is in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LandStatus {
    /// Waiting for the lander.
    Queued,
    /// Being rebased onto the current trunk head.
    Rebasing,
    /// On trunk.
    Landed,
    /// Could not land.
    Failed,
    /// Withdrawn.
    Cancelled,
}

impl LandStatus {
    /// True once the ticket will not change again.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            LandStatus::Landed | LandStatus::Failed | LandStatus::Cancelled
        )
    }

    fn from_proto(value: i32) -> Result<Self> {
        match pb::LandStatus::try_from(value) {
            Ok(pb::LandStatus::Queued) => Ok(LandStatus::Queued),
            Ok(pb::LandStatus::Rebasing) => Ok(LandStatus::Rebasing),
            Ok(pb::LandStatus::Landed) => Ok(LandStatus::Landed),
            Ok(pb::LandStatus::Failed) => Ok(LandStatus::Failed),
            Ok(pb::LandStatus::Cancelled) => Ok(LandStatus::Cancelled),
            _ => Err(rill_proto::ConvertError::BadEnum {
                field: "land_ticket.status",
                value,
            }
            .into()),
        }
    }
}

/// A landing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandTicket {
    /// Ticket id, used to poll status.
    pub id: String,
    /// The change being landed.
    pub change_id: ChangeId,
    /// Current status.
    pub status: LandStatus,
    /// When the land was requested.
    pub requested_at: Timestamp,
}

impl TryFrom<pb::LandTicket> for LandTicket {
    type Error = Error;

    fn try_from(t: pb::LandTicket) -> Result<Self> {
        Ok(LandTicket {
            id: t.id,
            change_id: id_from_proto("land_ticket.change_id", &t.change_id)?,
            status: LandStatus::from_proto(t.status)?,
            requested_at: timestamp_from_proto(t.requested_at),
        })
    }
}

/// A land ticket with its outcome, once known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandProgress {
    /// The ticket.
    pub ticket: LandTicket,
    /// Trunk position, once landed.
    pub seq: Option<TrunkSeq>,
    /// The landed version, once landed.
    pub landed_hash: Option<ChangeVersionHash>,
    /// Failure detail, if the landing failed.
    pub error: Option<String>,
}

impl LandProgress {
    /// Current status.
    pub fn status(&self) -> LandStatus {
        self.ticket.status
    }
}

impl TryFrom<pb::GetLandStatusResponse> for LandProgress {
    type Error = Error;

    fn try_from(r: pb::GetLandStatusResponse) -> Result<Self> {
        Ok(LandProgress {
            ticket: required("get_land_status.ticket", r.ticket)?.try_into()?,
            seq: r.seq.map(trunk_seq_from_proto),
            landed_hash: opt_hash_from_proto(
                "get_land_status.landed_hash",
                r.landed_hash.as_ref(),
            )?,
            error: r.error,
        })
    }
}

/// Something happened to a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEventKind {
    /// The status moved.
    StatusChanged(ReviewStatus),
    /// A vote was cast.
    Voted(LabelVote),
    /// A thread was opened, replied to or resolved.
    ThreadUpdated(Thread),
    /// A new version was added.
    NewVersion(ChangeVersionHash),
    /// A land ticket progressed.
    LandUpdate(LandTicket),
}

/// A review event with its subject and time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewEvent {
    /// The change.
    pub change_id: ChangeId,
    /// When it happened.
    pub at: Timestamp,
    /// What happened.
    pub kind: ReviewEventKind,
}

impl TryFrom<pb::ReviewEvent> for ReviewEvent {
    type Error = Error;

    fn try_from(e: pb::ReviewEvent) -> Result<Self> {
        let kind = match required("review_event.event", e.event)? {
            pb::review_event::Event::StatusChanged(s) => ReviewEventKind::StatusChanged(
                review_status_from_proto("review_event.status_changed", s)?,
            ),
            pb::review_event::Event::Voted(v) => ReviewEventKind::Voted(v.try_into()?),
            pb::review_event::Event::ThreadUpdated(t) => {
                ReviewEventKind::ThreadUpdated(t.try_into()?)
            }
            pb::review_event::Event::NewVersion(h) => {
                ReviewEventKind::NewVersion(hash_from_proto("review_event.new_version", &h)?)
            }
            pb::review_event::Event::LandUpdate(t) => ReviewEventKind::LandUpdate(t.try_into()?),
        };
        Ok(ReviewEvent {
            change_id: id_from_proto("review_event.change_id", &e.change_id)?,
            at: timestamp_from_proto(e.at),
            kind,
        })
    }
}
