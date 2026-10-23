//! Listing and deciding sampled audits.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_review::attention::{Audit, AuditStatus};
use rill_runtime::error::{Error, Result};
use rill_runtime::workers::calibration::escape;
use rill_types::RepoPathBuf;

use super::inbox::reader;
use super::AttentionSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};

fn audit_proto(opened: &Opened, a: Audit) -> pb::Audit {
    let ctx = &opened.repo;
    pb::Audit {
        landing: names::trunk_position(ctx, a.seq),
        change: names::change(ctx, &a.change),
        version: names::version(ctx, &a.version),
        assignees: a.assignees,
        risk: a.risk.as_str().into(),
        status: match a.status {
            AuditStatus::Pending => "pending",
            AuditStatus::Passed => "passed",
            AuditStatus::Failed => "failed",
        }
        .into(),
        sampled_at: timestamp_to_proto(a.sampled_at),
        decided_by: a.decided_by.map(pb::Principal::from).into(),
        decided_at: a.decided_at.map(timestamp_to_proto).unwrap_or_default(),
        note: a.note,
    }
}

impl AttentionSvc {
    pub(super) async fn list_audits_impl(
        &self,
        request: Call<pb::ListAuditsRequest>,
    ) -> Result<pb::ListAuditsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let audits = opened.repo.review().pending_audits().await?;
        Ok(pb::ListAuditsResponse {
            audits: audits
                .into_iter()
                .map(|a| audit_proto(&opened, a))
                .collect(),
        })
    }

    pub(super) async fn record_audit_impl(
        &self,
        request: Call<pb::RecordAuditRequest>,
    ) -> Result<pb::RecordAuditResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.landing).await?;
        opened.permit().review().await?;
        let seq = names::position_in(&opened.repo, &req.landing)?;
        let review = opened.repo.review();
        let audit = review
            .audit(seq)
            .await?
            .ok_or_else(|| Error::not_found(format!("no audit of trunk@{}", seq.0)))?;
        let me = reader(&opened).await;
        let assigned = audit.assignees.iter().any(|a| {
            (a == "owners" && me.repo_owner)
                || rill_policy::owners::is_entry(a, &me.approver.principal, &me.approver.teams)
        });
        if !assigned && opened.permit().admin().await.is_err() {
            return Err(Error::denied(format!(
                "the audit of trunk@{} is assigned to {}",
                seq.0,
                audit.assignees.join(", ")
            )));
        }
        let decided = review
            .decide_audit(seq, req.passed, opened.principal(), &req.note)
            .await?;
        if !req.passed {
            escape(&opened.repo, seq).await?;
            if req.revert {
                opened.permit().land().await?;
                let reason = format!("failed audit: {}", req.note);
                rill_lander::revert(
                    &opened.repo.lander,
                    seq,
                    &reason,
                    opened.principal().clone(),
                )
                .await?;
            }
        }
        tracing::info!(seq = seq.0, passed = req.passed, by = %opened.principal(), "audit decided");
        Ok(pb::RecordAuditResponse {
            audit: audit_proto(&opened, decided).into(),
        })
    }
}
