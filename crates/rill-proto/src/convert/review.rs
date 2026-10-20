//! Review state, labels, votes, threads and comments.

use rill_types::objects::review::CommentCategory;
use rill_types::{
    Comment, Label, LabelRange, LabelVote, Principal, ReviewState, ReviewStatus, Thread,
    ThreadAnchor,
};

use crate::convert::intent::risk_from_proto;
use crate::convert::primitives::{
    enum_from_proto, hash_from_proto, hash_to_proto, hashes_from_proto, id_from_proto, id_to_proto,
    ids_from_proto, opt_id_from_proto, path_from_proto, required, timestamp_from_proto,
    timestamp_to_proto, trunk_seq_from_proto, trunk_seq_to_proto,
};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

/// Clamp an `i32` vote value into the `i8` label range.
fn i8_from_proto(field: &'static str, value: i32) -> Result<i8> {
    i8::try_from(value).map_err(|_| ConvertError::OutOfRange {
        field,
        value: value as i64,
    })
}

impl From<&Label> for pb::Label {
    fn from(l: &Label) -> Self {
        pb::Label {
            name: l.name.clone(),
            min: l.range.min as i32,
            max: l.range.max as i32,
            copy_on_trivial_rebase: l.copy_on_trivial_rebase,
            max_requires_human: l.max_requires_human,
        }
    }
}

impl TryFrom<pb::Label> for Label {
    type Error = ConvertError;

    fn try_from(l: pb::Label) -> Result<Self> {
        Ok(Label {
            name: l.name,
            range: LabelRange {
                min: i8_from_proto("label.min", l.min)?,
                max: i8_from_proto("label.max", l.max)?,
            },
            copy_on_trivial_rebase: l.copy_on_trivial_rebase,
            max_requires_human: l.max_requires_human,
        })
    }
}

impl From<&LabelVote> for pb::LabelVote {
    fn from(v: &LabelVote) -> Self {
        pb::LabelVote {
            label: v.label.clone(),
            value: v.value as i32,
            voter: Some(pb::Principal::from(&v.voter)),
            version: hash_to_proto(&v.version),
            voted_at: timestamp_to_proto(v.voted_at),
            reason: v.reason.clone(),
        }
    }
}

impl TryFrom<pb::LabelVote> for LabelVote {
    type Error = ConvertError;

    fn try_from(v: pb::LabelVote) -> Result<Self> {
        Ok(LabelVote {
            label: v.label,
            value: i8_from_proto("label_vote.value", v.value)?,
            voter: required("label_vote.voter", v.voter)?.try_into()?,
            version: hash_from_proto("label_vote.version", &v.version)?,
            voted_at: timestamp_from_proto(v.voted_at),
            reason: v.reason,
        })
    }
}

impl From<&ThreadAnchor> for pb::ThreadAnchor {
    fn from(a: &ThreadAnchor) -> Self {
        let anchor = match a {
            ThreadAnchor::Change => pb::thread_anchor::Anchor::Change(pb::thread_anchor::Change {}),
            ThreadAnchor::Lines {
                version,
                path,
                start,
                end,
                context_hash,
            } => pb::thread_anchor::Anchor::Lines(pb::thread_anchor::Lines {
                version: hash_to_proto(version),
                path: path.to_string(),
                start: *start,
                end: *end,
                context_hash: context_hash.clone(),
            }),
            ThreadAnchor::File { version, path } => {
                pb::thread_anchor::Anchor::File(pb::thread_anchor::File {
                    version: hash_to_proto(version),
                    path: path.to_string(),
                })
            }
        };
        pb::ThreadAnchor {
            anchor: Some(anchor),
        }
    }
}

impl TryFrom<pb::ThreadAnchor> for ThreadAnchor {
    type Error = ConvertError;

    fn try_from(a: pb::ThreadAnchor) -> Result<Self> {
        match required("thread_anchor.anchor", a.anchor)? {
            pb::thread_anchor::Anchor::Change(_) => Ok(ThreadAnchor::Change),
            pb::thread_anchor::Anchor::Lines(l) => Ok(ThreadAnchor::Lines {
                version: hash_from_proto("thread_anchor.lines.version", &l.version)?,
                path: path_from_proto("thread_anchor.lines.path", &l.path)?,
                start: l.start,
                end: l.end,
                context_hash: l.context_hash,
            }),
            pb::thread_anchor::Anchor::File(f) => Ok(ThreadAnchor::File {
                version: hash_from_proto("thread_anchor.file.version", &f.version)?,
                path: path_from_proto("thread_anchor.file.path", &f.path)?,
            }),
        }
    }
}

impl From<CommentCategory> for pb::CommentCategory {
    fn from(c: CommentCategory) -> Self {
        match c {
            CommentCategory::Discussion => pb::CommentCategory::Discussion,
            CommentCategory::Bug => pb::CommentCategory::Bug,
            CommentCategory::Security => pb::CommentCategory::Security,
            CommentCategory::Style => pb::CommentCategory::Style,
            CommentCategory::TestGap => pb::CommentCategory::TestGap,
            CommentCategory::Question => pb::CommentCategory::Question,
            CommentCategory::Suggestion => pb::CommentCategory::Suggestion,
        }
    }
}

/// Decode a `CommentCategory` enum field.
pub fn comment_category_from_proto(field: &'static str, value: i32) -> Result<CommentCategory> {
    let c: pb::CommentCategory = enum_from_proto(field, value)?;
    match c {
        pb::CommentCategory::Discussion => Ok(CommentCategory::Discussion),
        pb::CommentCategory::Bug => Ok(CommentCategory::Bug),
        pb::CommentCategory::Security => Ok(CommentCategory::Security),
        pb::CommentCategory::Style => Ok(CommentCategory::Style),
        pb::CommentCategory::TestGap => Ok(CommentCategory::TestGap),
        pb::CommentCategory::Question => Ok(CommentCategory::Question),
        pb::CommentCategory::Suggestion => Ok(CommentCategory::Suggestion),
        pb::CommentCategory::Unspecified => Err(ConvertError::BadEnum { field, value }),
    }
}

/// Decode a confidence percentage, which must fit 0..=100.
pub fn confidence_from_proto(field: &'static str, value: u32) -> Result<u8> {
    if value > 100 {
        return Err(ConvertError::OutOfRange {
            field,
            value: value as i64,
        });
    }
    Ok(value as u8)
}

impl From<&Comment> for pb::Comment {
    fn from(c: &Comment) -> Self {
        pb::Comment {
            author: Some(pb::Principal::from(&c.author)),
            body: c.body.clone(),
            created_at: timestamp_to_proto(c.created_at),
            category: c.category.map(|c| pb::CommentCategory::from(c) as i32),
            confidence: c.confidence.map(u32::from),
        }
    }
}

impl TryFrom<pb::Comment> for Comment {
    type Error = ConvertError;

    fn try_from(c: pb::Comment) -> Result<Self> {
        Ok(Comment {
            author: required("comment.author", c.author)?.try_into()?,
            body: c.body,
            created_at: timestamp_from_proto(c.created_at),
            category: c
                .category
                .map(|v| comment_category_from_proto("comment.category", v))
                .transpose()?,
            confidence: c
                .confidence
                .map(|v| confidence_from_proto("comment.confidence", v))
                .transpose()?,
        })
    }
}

impl From<&Thread> for pb::Thread {
    fn from(t: &Thread) -> Self {
        pb::Thread {
            id: t.id.clone(),
            anchor: Some(pb::ThreadAnchor::from(&t.anchor)),
            comments: t.comments.iter().map(pb::Comment::from).collect(),
            resolved: t.resolved,
            to: t.to.as_ref().map(pb::Principal::from),
        }
    }
}

impl TryFrom<pb::Thread> for Thread {
    type Error = ConvertError;

    fn try_from(t: pb::Thread) -> Result<Self> {
        Ok(Thread {
            id: t.id,
            anchor: required("thread.anchor", t.anchor)?.try_into()?,
            comments: t
                .comments
                .into_iter()
                .map(Comment::try_from)
                .collect::<Result<_>>()?,
            resolved: t.resolved,
            to: t.to.map(Principal::try_from).transpose()?,
        })
    }
}

impl From<ReviewStatus> for pb::ReviewStatus {
    fn from(s: ReviewStatus) -> Self {
        match s {
            ReviewStatus::Open => pb::ReviewStatus::Open,
            ReviewStatus::Landable => pb::ReviewStatus::Landable,
            ReviewStatus::Landing => pb::ReviewStatus::Landing,
            ReviewStatus::Landed => pb::ReviewStatus::Landed,
            ReviewStatus::Abandoned => pb::ReviewStatus::Abandoned,
        }
    }
}

/// Decode a `ReviewStatus` enum field.
pub fn review_status_from_proto(field: &'static str, value: i32) -> Result<ReviewStatus> {
    let s: pb::ReviewStatus = enum_from_proto(field, value)?;
    match s {
        pb::ReviewStatus::Open => Ok(ReviewStatus::Open),
        pb::ReviewStatus::Landable => Ok(ReviewStatus::Landable),
        pb::ReviewStatus::Landing => Ok(ReviewStatus::Landing),
        pb::ReviewStatus::Landed => Ok(ReviewStatus::Landed),
        pb::ReviewStatus::Abandoned => Ok(ReviewStatus::Abandoned),
        pb::ReviewStatus::Unspecified => Err(ConvertError::BadEnum { field, value }),
    }
}

impl From<&ReviewState> for pb::ReviewState {
    fn from(r: &ReviewState) -> Self {
        pb::ReviewState {
            change_id: id_to_proto(&r.change_id),
            versions: r.versions.iter().map(hash_to_proto).collect(),
            status: pb::ReviewStatus::from(r.status) as i32,
            intent: r.intent.as_ref().map(id_to_proto),
            owner: Some(pb::Principal::from(&r.owner)),
            votes: r.votes.iter().map(pb::LabelVote::from).collect(),
            threads: r.threads.iter().map(pb::Thread::from).collect(),
            risk: pb::RiskClass::from(r.risk) as i32,
            risk_reasons: r.risk_reasons.clone(),
            stack: r.stack.iter().map(id_to_proto).collect(),
            landed_seq: r.landed_seq.map(trunk_seq_to_proto),
            created_at: timestamp_to_proto(r.created_at),
            updated_at: timestamp_to_proto(r.updated_at),
            extra: r.extra.clone(),
        }
    }
}

impl From<ReviewState> for pb::ReviewState {
    fn from(r: ReviewState) -> Self {
        pb::ReviewState::from(&r)
    }
}

impl TryFrom<pb::ReviewState> for ReviewState {
    type Error = ConvertError;

    fn try_from(r: pb::ReviewState) -> Result<Self> {
        Ok(ReviewState {
            change_id: id_from_proto("review_state.change_id", &r.change_id)?,
            versions: hashes_from_proto("review_state.versions", &r.versions)?,
            status: review_status_from_proto("review_state.status", r.status)?,
            intent: opt_id_from_proto("review_state.intent", r.intent.as_ref())?,
            owner: required("review_state.owner", r.owner)?.try_into()?,
            votes: r
                .votes
                .into_iter()
                .map(LabelVote::try_from)
                .collect::<Result<_>>()?,
            threads: r
                .threads
                .into_iter()
                .map(Thread::try_from)
                .collect::<Result<_>>()?,
            risk: risk_from_proto("review_state.risk", r.risk)?,
            risk_reasons: r.risk_reasons,
            stack: ids_from_proto("review_state.stack", &r.stack)?,
            landed_seq: r.landed_seq.map(trunk_seq_from_proto),
            created_at: timestamp_from_proto(r.created_at),
            updated_at: timestamp_from_proto(r.updated_at),
            extra: r.extra,
        })
    }
}
