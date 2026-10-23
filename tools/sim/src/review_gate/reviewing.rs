//! The fleet reviewing a gate change, the policy deciding, and a person
//! stepping in where it needs one.

use rill_review::agents::{Budget, Finding, ReviewResult};
use rill_review_runner::backend::{Backend, Fake};
use rill_sdk::{ReviewFile, ReviewJob, ReviewPacket, ReviewerProfile};
use rill_types::{Principal, RepoPathBuf, TrunkSeq};

use super::world::{GateChange, GateWorld};
use super::GateReport;

impl GateWorld {
    /// What a reviewer is sent for `change`, as the server builds it.
    async fn job(&self, change: &GateChange) -> ReviewJob {
        let ctx = self
            .service
            .review_context(&change.id, change.version, Budget::default())
            .await
            .expect("the change has a context");
        let files = ctx
            .diffs
            .iter()
            .map(|d| {
                let hunks = match (&d.before, &d.after) {
                    (None, Some(after)) if d.hunks.is_empty() => rill_core::diff_lines(b"", after),
                    _ => d.hunks.clone(),
                };
                ReviewFile {
                    path: d.path.to_string(),
                    unified: String::from_utf8_lossy(&rill_core::to_unified(
                        d.path.as_str(),
                        &hunks,
                    ))
                    .into_owned(),
                    added: d.before.is_none(),
                    deleted: d.after.is_none(),
                }
            })
            .collect();
        ReviewJob {
            change: change.id,
            version: change.version,
            reason: "submitted".into(),
            profile: ReviewerProfile {
                max_vote: 1,
                ..ReviewerProfile::default()
            },
            prompt: String::new(),
            packet: ReviewPacket {
                description: ctx.change.message.clone(),
                author: Some(change.author.clone()),
                risk: String::new(),
                intent_title: String::new(),
                intent_body: String::new(),
                files,
                truncated: ctx.truncated,
            },
        }
    }

    /// Review `change` with the fleet, then land it on agents, with a
    /// person, or send it back, counting the outcome in `report`.
    pub(super) async fn review(&self, fleet: &Fake, change: &GateChange, report: &mut GateReport) {
        let job = self.job(change).await;
        let mut reviewers = self.reviewers.clone();
        if change.sensitive {
            reviewers.push(self.security.clone());
        }
        let mut blocked = false;
        for (i, reviewer) in reviewers.iter().enumerate() {
            // Each reviewer draws on its own: a different seed per reviewer.
            let mut own = job.clone();
            own.version = rill_types::ObjectHash::digest(
                rill_types::ObjectKind::Derived,
                format!("{}:{i}", change.version).as_bytes(),
            );
            let report = fleet.review(&own).await.expect("the fake never fails");
            blocked |= report.vote < 0;
            let findings = report
                .findings
                .into_iter()
                .map(|f| Finding {
                    path: f.path.and_then(|p| RepoPathBuf::new(p).ok()),
                    lines: f.lines,
                    category: f.category,
                    confidence: f.confidence,
                    body: f.body,
                })
                .collect();
            self.service
                .post_review_result(
                    &change.id,
                    reviewer,
                    &ReviewResult {
                        version: change.version,
                        findings,
                        vote: report.vote,
                        summary: report.summary,
                    },
                )
                .await
                .expect("the version is current");
        }
        if change.defect {
            report.defects += 1;
        }
        if blocked {
            report.blocked += 1;
            if change.defect {
                report.caught_by_reviewers += 1;
            }
            return;
        }
        let ci = Principal::service("ci");
        self.service
            .vote(&change.id, &ci, "Verified", 1, None)
            .await
            .expect("verification votes");
        let decision = self.service.evaluate(&change.id).await.expect("evaluates");
        if decision.allowed {
            report.agent_only += 1;
            if change.defect {
                let seq = TrunkSeq(report.agent_only as u64);
                let risk = self.service.get(&change.id).await.expect("exists").risk;
                if rill_review::attention::sampled(seq, self.audit_permille, risk) {
                    report.caught_by_audits += 1;
                } else {
                    report.escaped += 1;
                }
            }
            return;
        }
        // A person steps in: the author's sponsor reviews and owns it.
        report.needed_human += 1;
        for (label, value) in [("Review", 2), ("Owner", 1)] {
            self.service
                .vote(&change.id, &change.sponsor, label, value, None)
                .await
                .expect("people vote");
        }
        if change.defect {
            // Conservatively, a defect the fleet missed is not assumed to
            // be caught by the person who approves it.
            report.escaped += 1;
        }
    }
}
