//! A deterministic reviewer for tests and simulations: it flags lines
//! carrying [`DEFECT_MARKER`], catching each with a fixed probability
//! derived from the version and line, and wrongly blocks a fixed share of
//! changes. It resolves conflicts by keeping every side.

use async_trait::async_trait;
use rill_sdk::{ReviewFinding, ReviewJob, ReviewReport};
use rill_types::objects::review::CommentCategory;

use super::{vote_for, Backend, ResolveOutcome, ResolveTask};
use crate::error::Result;
use crate::packet::added_lines;

/// The marker a seeded defect carries in an added line.
pub const DEFECT_MARKER: &str = "RILL-DEFECT";

/// The fake backend.
#[derive(Debug, Clone, Copy)]
pub struct Fake {
    /// Seeded defects caught per thousand.
    pub detect_permille: u32,
    /// Changes wrongly blocked per thousand.
    pub false_positive_permille: u32,
}

/// A stable draw in 0..1000 for `key`.
fn draw(key: &str) -> u32 {
    let h = blake3::hash(key.as_bytes());
    u32::from_le_bytes(h.as_bytes()[..4].try_into().expect("four bytes")) % 1000
}

impl Fake {
    /// The findings for `job`.
    pub fn findings(&self, job: &ReviewJob) -> Vec<ReviewFinding> {
        let version = job.version.to_hex();
        let mut out = Vec::new();
        let mut first_added = None;
        for f in &job.packet.files {
            for (line, text) in added_lines(&f.unified) {
                first_added.get_or_insert((f.path.clone(), line));
                let Some(at) = text.find(DEFECT_MARKER) else {
                    continue;
                };
                if draw(&format!("{version}:{}:{line}", f.path)) >= self.detect_permille {
                    continue;
                }
                out.push(ReviewFinding {
                    path: Some(f.path.clone()),
                    lines: Some((line, line)),
                    category: CommentCategory::Bug,
                    confidence: 90,
                    body: format!(
                        "Seeded defect: {}",
                        text[at + DEFECT_MARKER.len()..].trim_matches([':', ' '])
                    ),
                });
            }
        }
        if out.is_empty() && draw(&format!("fp:{version}")) < self.false_positive_permille {
            if let Some((path, line)) = first_added {
                out.push(ReviewFinding {
                    path: Some(path),
                    lines: Some((line, line)),
                    category: CommentCategory::Bug,
                    confidence: 75,
                    body: "This looks wrong (a deliberate false positive).".into(),
                });
            }
        }
        out
    }
}

#[async_trait]
impl Backend for Fake {
    async fn review(&self, job: &ReviewJob) -> Result<ReviewReport> {
        let findings = self.findings(job);
        let vote = vote_for(&findings, job.profile.max_vote);
        Ok(ReviewReport {
            summary: format!("{} finding(s)", findings.len()),
            findings,
            vote,
            model: "fake".into(),
        })
    }

    async fn resolve(&self, task: &ResolveTask) -> Result<ResolveOutcome> {
        let mut files = Vec::with_capacity(task.files.len());
        for f in &task.files {
            let Some(sides) = f.sides.iter().cloned().collect::<Option<Vec<_>>>() else {
                return Ok(ResolveOutcome::Declined(format!(
                    "{} has a deleted or binary side",
                    f.path
                )));
            };
            let mut kept: Vec<String> = Vec::new();
            for side in sides {
                if !kept.contains(&side) {
                    kept.push(side);
                }
            }
            files.push((f.path.clone(), Some(kept.concat())));
        }
        Ok(ResolveOutcome::Resolved {
            files,
            rationale: "kept every side".into(),
            model: "fake".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use rill_sdk::{ReviewFile, ReviewPacket, ReviewerProfile};
    use rill_types::{ChangeId, ObjectHash, ObjectKind};

    use super::*;

    fn job(n: u32, lines: &str) -> ReviewJob {
        ReviewJob {
            change: ChangeId::new(),
            version: ObjectHash::digest(ObjectKind::Derived, &n.to_le_bytes()),
            reason: "submitted".into(),
            profile: ReviewerProfile {
                max_vote: 1,
                ..ReviewerProfile::default()
            },
            prompt: String::new(),
            packet: ReviewPacket {
                description: String::new(),
                author: None,
                risk: "normal".into(),
                intent_title: String::new(),
                intent_body: String::new(),
                files: vec![ReviewFile {
                    path: "src/a.rs".into(),
                    unified: format!("--- /dev/null\n+++ b/src/a.rs\n@@ -0,0 +1,2 @@\n{lines}"),
                    added: true,
                    deleted: false,
                }],
                truncated: false,
            },
        }
    }

    #[tokio::test]
    async fn seeded_defects_are_caught_at_the_configured_rate() {
        let fake = Fake {
            detect_permille: 900,
            false_positive_permille: 0,
        };
        let mut caught = 0;
        for n in 0..1000 {
            let report = fake
                .review(&job(n, "+ok\n+x // RILL-DEFECT: off by one\n"))
                .await
                .unwrap();
            if report.vote < 0 {
                caught += 1;
                assert_eq!(report.findings[0].lines, Some((2, 2)));
                assert_eq!(report.findings[0].body, "Seeded defect: off by one");
            }
        }
        assert!((850..=950).contains(&caught), "{caught}");
        let again = fake.review(&job(7, "+x RILL-DEFECT\n")).await.unwrap();
        assert_eq!(
            again,
            fake.review(&job(7, "+x RILL-DEFECT\n")).await.unwrap()
        );
    }

    #[tokio::test]
    async fn clean_changes_pass_unless_drawn_as_false_positives() {
        let fake = Fake {
            detect_permille: 1000,
            false_positive_permille: 100,
        };
        let mut blocked = 0;
        for n in 0..1000 {
            if fake.review(&job(n, "+ok\n+ok\n")).await.unwrap().vote < 0 {
                blocked += 1;
            }
        }
        assert!((60..=140).contains(&blocked), "{blocked}");
    }
}
