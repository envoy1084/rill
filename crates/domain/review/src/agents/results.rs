//! A reviewer's result: checked against the reviewer's limits, then posted
//! as threads and a vote by that reviewer.
//!
//! A result is data, never instructions: whatever a reviewed diff
//! persuades a model to say, the server only posts findings on paths the
//! change touches and a vote within the reviewer's maximum, on the
//! version it reviewed.

use std::collections::BTreeSet;

use rill_policy::submit::REVIEW_LABEL;
use rill_types::{ChangeId, ChangeVersionHash, Principal, RepoPathBuf, ThreadAnchor};

use super::reviewer::Finding;
use crate::error::{Error, Result};
use crate::service::ReviewService;

/// Findings one result may carry.
pub const MAX_FINDINGS: usize = 50;
/// Bytes of one finding's body, or of the summary.
pub const MAX_BODY: usize = 8 * 1024;
/// Findings below this confidence are recorded but not posted.
const MIN_CONFIDENCE_TO_POST: u8 = 30;

/// What a reviewer reports for one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewResult {
    pub version: ChangeVersionHash,
    pub findings: Vec<Finding>,
    /// The `Review` vote; 0 casts none.
    pub vote: i8,
    pub summary: String,
}

/// What was posted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostedResult {
    pub threads: Vec<String>,
    pub vote: i8,
}

/// Check `result` against a reviewer whose votes reach `max_vote`, for a
/// version touching `touched`.
pub fn check_result(
    result: &ReviewResult,
    max_vote: i8,
    touched: &BTreeSet<RepoPathBuf>,
) -> Result<()> {
    let bad = |m: String| Err(Error::BadResult(m));
    if result.vote.unsigned_abs() > max_vote.unsigned_abs() {
        return bad(format!(
            "vote {} exceeds the reviewer's maximum of {max_vote}",
            result.vote
        ));
    }
    if result.findings.len() > MAX_FINDINGS {
        return bad(format!(
            "{} findings; at most {MAX_FINDINGS}",
            result.findings.len()
        ));
    }
    if result.summary.len() > MAX_BODY {
        return bad(format!("the summary is over {MAX_BODY} bytes"));
    }
    for f in &result.findings {
        if f.body.trim().is_empty() || f.body.len() > MAX_BODY {
            return bad(format!("a finding's body must be 1..={MAX_BODY} bytes"));
        }
        if f.confidence > 100 {
            return bad(format!("confidence {} is over 100", f.confidence));
        }
        match (&f.path, f.lines) {
            (Some(path), _) if !touched.contains(path) => {
                return bad(format!("{path} is not a path the change touches"));
            }
            (None, Some(_)) => return bad("a line range needs a path".into()),
            (_, Some((start, end))) if start == 0 || end < start => {
                return bad(format!("bad line range {start}..={end}"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn anchor_for(version: ChangeVersionHash, finding: &Finding) -> ThreadAnchor {
    match (&finding.path, finding.lines) {
        (Some(path), Some((start, end))) => ThreadAnchor::Lines {
            version,
            path: path.clone(),
            start,
            end,
            context_hash: String::new(),
        },
        (Some(path), None) => ThreadAnchor::File {
            version,
            path: path.clone(),
        },
        (None, _) => ThreadAnchor::Change,
    }
}

impl ReviewService {
    /// The paths `version` touches.
    pub async fn touched_paths(
        &self,
        version: &ChangeVersionHash,
    ) -> Result<BTreeSet<RepoPathBuf>> {
        Ok(self
            .version_facts(version)
            .await?
            .paths
            .into_iter()
            .map(|p| p.path)
            .collect())
    }

    /// Post a checked `result` as `reviewer`: refused when the change has
    /// moved past the reviewed version, else its findings become threads
    /// and its vote is cast on that version.
    pub async fn post_review_result(
        &self,
        change: &ChangeId,
        reviewer: &Principal,
        result: &ReviewResult,
    ) -> Result<PostedResult> {
        let stale = || Error::StaleVersion {
            change_id: *change,
            version: result.version,
        };
        if self.get(change).await?.current_version() != Some(result.version) {
            return Err(stale());
        }
        let mut posted = PostedResult::default();
        for f in &result.findings {
            if f.confidence < MIN_CONFIDENCE_TO_POST {
                continue;
            }
            let id = self
                .comment(
                    change,
                    reviewer,
                    anchor_for(result.version, f),
                    f.body.clone(),
                    Some(f.category),
                    Some(f.confidence),
                    None,
                )
                .await?;
            posted.threads.push(id);
        }
        if result.vote != 0 {
            let reason = (!result.summary.is_empty()).then(|| result.summary.clone());
            self.vote_at(
                change,
                reviewer,
                REVIEW_LABEL,
                result.vote,
                reason,
                Some(result.version),
            )
            .await?;
            posted.vote = result.vote;
        }
        Ok(posted)
    }
}

#[cfg(test)]
mod tests {
    use rill_types::objects::review::CommentCategory;
    use rill_types::{ObjectHash, ObjectKind};

    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn a_result_stays_within_the_reviewers_limits() {
        let touched = BTreeSet::from([p("src/a.rs")]);
        let ok = ReviewResult {
            version: ObjectHash::digest(ObjectKind::Derived, b"v"),
            findings: vec![Finding::at(
                p("src/a.rs"),
                3,
                4,
                CommentCategory::Bug,
                80,
                "off by one",
            )],
            vote: -1,
            summary: String::new(),
        };
        check_result(&ok, 1, &touched).unwrap();
        let refused = |r: ReviewResult| check_result(&r, 1, &touched).is_err();
        assert!(refused(ReviewResult {
            vote: -2,
            ..ok.clone()
        }));
        let mut outside = ok.clone();
        outside.findings[0].path = Some(p("secrets/key"));
        assert!(refused(outside), "a path the change does not touch");
        let mut backwards = ok.clone();
        backwards.findings[0].lines = Some((5, 4));
        assert!(refused(backwards));
        let mut flood = ok.clone();
        flood.findings = vec![ok.findings[0].clone(); MAX_FINDINGS + 1];
        assert!(refused(flood));
    }
}
