//! The reviewer fleet: jobs, findings and runs.

use rill_proto::convert::{comment_category_from_proto, timestamp_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, ChangeVersionHash, Principal, Timestamp};

use crate::error::Result;
use crate::names;

/// A held job: pass it back to renew, complete or fail the job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The opaque lease.
    pub token: Vec<u8>,
    /// When it runs out unless renewed.
    pub until: Timestamp,
}

/// How a reviewer reviews.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewerProfile {
    /// The model's family (`claude`, …).
    pub model_family: String,
    /// The model it runs.
    pub model: String,
    /// The prompt's repository path.
    pub prompt: String,
    /// The largest `Review` vote it may cast, either way.
    pub max_vote: i8,
    /// Reviews it may run per day in a repository.
    pub runs_per_day: Option<u64>,
}

impl From<pb::ReviewerProfile> for ReviewerProfile {
    fn from(p: pb::ReviewerProfile) -> Self {
        ReviewerProfile {
            model_family: p.model_family,
            model: p.model,
            prompt: p.prompt,
            max_vote: p.max_vote.clamp(-2, 2) as i8,
            runs_per_day: p.runs_per_day,
        }
    }
}

/// One file of a review packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFile {
    /// The path.
    pub path: String,
    /// Unified diff; empty for binary, oversized or dropped files.
    pub unified: String,
    /// The file is new.
    pub added: bool,
    /// The file was deleted.
    pub deleted: bool,
}

/// The change as a reviewer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPacket {
    /// The change's description.
    pub description: String,
    /// Who wrote the version.
    pub author: Option<Principal>,
    /// `trivial`, `low`, `normal`, `sensitive` or `critical`.
    pub risk: String,
    /// The linked intent's title.
    pub intent_title: String,
    /// The linked intent's body.
    pub intent_body: String,
    /// The touched files.
    pub files: Vec<ReviewFile>,
    /// Diffs were dropped to fit the server's budget.
    pub truncated: bool,
}

/// A review to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewJob {
    /// The change.
    pub change: ChangeId,
    /// The version.
    pub version: ChangeVersionHash,
    /// Why it was queued.
    pub reason: String,
    /// The reviewer's profile.
    pub profile: ReviewerProfile,
    /// The prompt file's text on trunk; empty when there is none.
    pub prompt: String,
    /// What to review.
    pub packet: ReviewPacket,
}

impl TryFrom<pb::ReviewJob> for ReviewJob {
    type Error = crate::Error;

    fn try_from(j: pb::ReviewJob) -> Result<Self> {
        let p = j.packet.into_option().unwrap_or_default();
        Ok(ReviewJob {
            change: names::change_id(&j.change)?,
            version: names::version_hash(&j.version)?,
            reason: j.reason,
            profile: j.profile.into_option().unwrap_or_default().into(),
            prompt: j.prompt,
            packet: ReviewPacket {
                description: p.description,
                author: p
                    .author
                    .into_option()
                    .map(Principal::try_from)
                    .transpose()?,
                risk: p.risk,
                intent_title: p.intent_title,
                intent_body: p.intent_body,
                files: p
                    .files
                    .into_iter()
                    .map(|f| ReviewFile {
                        path: f.path,
                        unified: f.unified,
                        added: f.added,
                        deleted: f.deleted,
                    })
                    .collect(),
                truncated: p.truncated,
            },
        })
    }
}

/// A conflicted version to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveJob {
    /// The change.
    pub change: ChangeId,
    /// The version.
    pub version: ChangeVersionHash,
    /// The conflicted paths.
    pub paths: Vec<String>,
    /// The prompt text.
    pub prompt: String,
}

impl TryFrom<pb::ResolveJob> for ResolveJob {
    type Error = crate::Error;

    fn try_from(j: pb::ResolveJob) -> Result<Self> {
        Ok(ResolveJob {
            change: names::change_id(&j.change)?,
            version: names::version_hash(&j.version)?,
            paths: j.paths,
            prompt: j.prompt,
        })
    }
}

/// One observation a reviewer reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFinding {
    /// A touched path; `None` for the whole change.
    pub path: Option<String>,
    /// One-based inclusive lines; `None` for the whole file.
    pub lines: Option<(u32, u32)>,
    /// What kind of finding.
    pub category: CommentCategory,
    /// 0-100.
    pub confidence: u8,
    /// The comment.
    pub body: String,
}

impl From<&ReviewFinding> for pb::ReviewFinding {
    fn from(f: &ReviewFinding) -> Self {
        let (start_line, end_line) = f.lines.unwrap_or((0, 0));
        pb::ReviewFinding {
            path: f.path.clone().unwrap_or_default(),
            start_line,
            end_line,
            category: pb::CommentCategory::from(f.category).into(),
            confidence: f.confidence.into(),
            body: f.body.clone(),
        }
    }
}

/// What a reviewer reports for its job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewReport {
    /// Findings to post.
    pub findings: Vec<ReviewFinding>,
    /// The `Review` vote; 0 casts none.
    pub vote: i8,
    /// A summary, kept as the vote reason.
    pub summary: String,
    /// The model it runs.
    pub model: String,
}

/// Where a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewRunStatus {
    /// Waiting for a runner.
    Queued,
    /// Posted.
    Done,
    /// Failed or refused.
    Failed,
    /// The change moved on first.
    Stale,
    /// Not run: over budget or not a reviewer.
    Skipped,
}

impl ReviewRunStatus {
    /// The status as a word.
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewRunStatus::Queued => "queued",
            ReviewRunStatus::Done => "done",
            ReviewRunStatus::Failed => "failed",
            ReviewRunStatus::Stale => "stale",
            ReviewRunStatus::Skipped => "skipped",
        }
    }
}

/// One reviewer's (or resolver's) run on one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRun {
    /// `agent:<name>`.
    pub reviewer: String,
    /// The version.
    pub version: ChangeVersionHash,
    /// Why it was queued.
    pub reason: String,
    /// Where it stands.
    pub status: ReviewRunStatus,
    /// When it was queued.
    pub queued_at: Timestamp,
    /// When it finished.
    pub finished_at: Option<Timestamp>,
    /// Findings to post.
    pub findings: u32,
    /// The vote.
    pub vote: i8,
    /// The model it runs.
    pub model: String,
    /// The summary, or why it failed or was skipped.
    pub detail: String,
    /// `review` or `resolve`.
    pub kind: String,
}

impl TryFrom<pb::ReviewRun> for ReviewRun {
    type Error = crate::Error;

    fn try_from(r: pb::ReviewRun) -> Result<Self> {
        let status = match rill_proto::convert::known(r.status) {
            Ok(pb::ReviewRunStatus::Queued) => ReviewRunStatus::Queued,
            Ok(pb::ReviewRunStatus::Done) => ReviewRunStatus::Done,
            Ok(pb::ReviewRunStatus::Stale) => ReviewRunStatus::Stale,
            Ok(pb::ReviewRunStatus::Skipped) => ReviewRunStatus::Skipped,
            _ => ReviewRunStatus::Failed,
        };
        Ok(ReviewRun {
            reviewer: r.reviewer,
            version: names::version_hash(&r.version)?,
            reason: r.reason,
            status,
            queued_at: timestamp_from_proto(r.queued_at),
            finished_at: (r.finished_at != 0).then(|| timestamp_from_proto(r.finished_at)),
            findings: r.findings,
            vote: r.vote.clamp(-2, 2) as i8,
            model: r.model,
            detail: r.detail,
            kind: r.kind,
        })
    }
}

/// Parse a category the way findings name them (`bug`, `security`, …).
pub fn parse_category(name: &str) -> Option<CommentCategory> {
    let c = match name.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
        "discussion" => pb::CommentCategory::Discussion,
        "bug" => pb::CommentCategory::Bug,
        "security" => pb::CommentCategory::Security,
        "style" => pb::CommentCategory::Style,
        "test_gap" | "testgap" => pb::CommentCategory::TestGap,
        "question" => pb::CommentCategory::Question,
        "suggestion" => pb::CommentCategory::Suggestion,
        _ => return None,
    };
    comment_category_from_proto("category", c).ok()
}

/// A reviewer's track record in a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerStats {
    /// `agent:<name>`.
    pub reviewer: String,
    /// Permille.
    pub trust: u32,
    /// Outcomes counted.
    pub outcomes: u32,
    /// Approved changes that landed.
    pub approvals: u64,
    /// Of those, reverted, blamed or failed in audit.
    pub escapes: u64,
    /// Blocking findings the author fixed.
    pub blocks: u64,
    /// Blocking findings resolved without a change.
    pub dismissed: u64,
    /// Routed nothing; its approvals do not count.
    pub demoted: bool,
    /// Why it was demoted or reinstated last.
    pub note: String,
    /// When.
    pub since: Option<Timestamp>,
}

impl From<pb::ReviewerStats> for ReviewerStats {
    fn from(s: pb::ReviewerStats) -> Self {
        ReviewerStats {
            reviewer: s.reviewer,
            trust: s.trust,
            outcomes: s.outcomes,
            approvals: s.approvals,
            escapes: s.escapes,
            blocks: s.blocks,
            dismissed: s.dismissed,
            demoted: s.demoted,
            note: s.note,
            since: (s.since != 0).then(|| timestamp_from_proto(s.since)),
        }
    }
}
