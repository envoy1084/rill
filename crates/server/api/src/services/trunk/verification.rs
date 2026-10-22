//! Landings' verification: reading it, runners reporting it, and
//! bisecting a break.
//!
//! Verification results come from two sources: the post-land verifier's
//! own records (`rill_lander::postland`) and reports external runners file
//! through `ReportVerification`, kept per sequence in the misc keyspace.

use bytes::Bytes;
use rill_lander::postland::{record_at, VerificationRecord, VerificationStatus as PostStatus};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{Principal, RepoPathBuf, Timestamp, TrunkSeq};
use serde::{Deserialize, Serialize};

use super::TrunkSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// A verification report filed by an external runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Report {
    target: String,
    status: i32,
    reporter: Principal,
    reported_at: Timestamp,
    details: Option<String>,
    artifact: Option<Vec<u8>>,
}

fn reports_key(seq: TrunkSeq) -> String {
    format!("verify_reports:{}", seq.get())
}

async fn load_reports(ctx: &RepoContext, seq: TrunkSeq) -> Result<Vec<Report>> {
    match ctx.repo.get_misc(&reports_key(seq)).await? {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::internal(e.to_string())),
        None => Ok(Vec::new()),
    }
}

async fn store_reports(ctx: &RepoContext, seq: TrunkSeq, reports: &[Report]) -> Result<()> {
    let bytes = serde_json::to_vec(reports).map_err(|e| Error::internal(e.to_string()))?;
    ctx.repo
        .set_misc(&reports_key(seq), Bytes::from(bytes))
        .await?;
    Ok(())
}

fn post_status_proto(record: &VerificationRecord) -> pb::VerificationStatus {
    match record.status {
        PostStatus::Green => pb::VerificationStatus::Passed,
        PostStatus::Red if record.inferred && record.culprit.is_some_and(|c| c != record.seq) => {
            pb::VerificationStatus::Inherited
        }
        PostStatus::Red | PostStatus::Flaky => pb::VerificationStatus::Failed,
    }
}

/// Combine the post-land record and external reports for `seq`.
pub(super) async fn verification(ctx: &RepoContext, seq: TrunkSeq) -> Result<pb::Verification> {
    let record = record_at(&ctx.repo, seq).await?;
    let external = load_reports(ctx, seq).await?;
    let mut reports: Vec<pb::VerificationReport> = Vec::with_capacity(external.len() + 1);
    if let Some(r) = &record {
        reports.push(pb::VerificationReport {
            target: "postland".into(),
            status: post_status_proto(r).into(),
            reporter: pb::Principal::from(rill_lander::revert::lander_principal()).into(),
            reported_at: timestamp_to_proto(r.at),
            details: Some(r.details.clone()),
            artifact: None,
        });
    }
    for r in &external {
        reports.push(pb::VerificationReport {
            target: r.target.clone(),
            status: r.status.into(),
            reporter: pb::Principal::from(&r.reporter).into(),
            reported_at: timestamp_to_proto(r.reported_at),
            details: r.details.clone(),
            artifact: r.artifact.as_ref().map(|a| Bytes::copy_from_slice(a)),
        });
    }
    let status = aggregate(reports.iter().map(|r| r.status.to_i32()));
    Ok(pb::Verification {
        landing: names::trunk_position(ctx, seq),
        position: seq.get(),
        status: status.into(),
        culprit: record.as_ref().and_then(|r| r.culprit.map(TrunkSeq::get)),
        reports,
    })
}

/// Worst-wins aggregation: any failure fails, otherwise running or
/// pending wins over passed, and no reports means pending.
fn aggregate(statuses: impl Iterator<Item = i32>) -> pb::VerificationStatus {
    use pb::VerificationStatus as S;
    let mut result: Option<S> = None;
    for status in statuses {
        let s = buffa::EnumValue::<S>::from(status)
            .as_known()
            .unwrap_or(S::Unspecified);
        let rank = |s: S| match s {
            S::Failed | S::Inherited => 5,
            S::Running => 4,
            S::Pending | S::Unspecified => 3,
            S::Skipped => 2,
            S::Passed => 1,
        };
        result = Some(match result {
            Some(r) if rank(r) >= rank(s) => r,
            _ => s,
        });
    }
    result.unwrap_or(S::Pending)
}

impl TrunkSvc {
    pub(super) async fn get_verification_impl(
        &self,
        request: Call<pb::GetVerificationRequest>,
    ) -> Result<pb::GetVerificationResponse> {
        let (opened, seq) = self.landing(&request, &request.get_ref().landing).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        Ok(pb::GetVerificationResponse {
            verification: verification(&opened.repo, seq).await?.into(),
        })
    }

    pub(super) async fn report_verification_impl(
        &self,
        request: Call<pb::ReportVerificationRequest>,
    ) -> Result<pb::ReportVerificationResponse> {
        let (opened, seq) = self.landing(&request, &request.get_ref().landing).await?;
        opened.permit().land().await?;
        let req = request.get_ref();
        let ctx = &opened.repo;
        if req.target.is_empty() {
            return Err(Error::invalid("target is required"));
        }
        req.status
            .as_known()
            .filter(|s| *s != pb::VerificationStatus::Unspecified)
            .ok_or_else(|| Error::invalid(format!("unknown verification status {}", req.status)))?;
        let mut reports = load_reports(ctx, seq).await?;
        // One report per (target, reporter): a re-run replaces its earlier
        // result rather than appending a contradictory one.
        reports.retain(|r| !(r.target == req.target && r.reporter.id == opened.principal().id));
        reports.push(Report {
            target: req.target.clone(),
            status: req.status.to_i32(),
            reporter: opened.principal().clone(),
            reported_at: Timestamp::now(),
            details: req.details.clone(),
            artifact: req.artifact.as_ref().map(|a| a.to_vec()),
        });
        store_reports(ctx, seq, &reports).await?;
        Ok(pb::ReportVerificationResponse {
            verification: verification(ctx, seq).await?.into(),
        })
    }

    pub(super) async fn bisect_impl(
        &self,
        request: Call<pb::BisectRequest>,
    ) -> Result<pb::BisectResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().land().await?;
        let req = request.get_ref();
        let ctx = &opened.repo;
        let postland = ctx.postland.clone().ok_or_else(|| {
            Error::precondition("post-land verification is disabled; enable `postland` to bisect")
        })?;
        if req.to <= req.from {
            return Err(Error::invalid("to must be greater than from"));
        }
        let culprit = postland
            .bisect(TrunkSeq(req.from), TrunkSeq(req.to))
            .await?;
        let mut tested = Vec::new();
        for seq in req.from + 1..=req.to {
            if record_at(&ctx.repo, TrunkSeq(seq)).await?.is_some() {
                tested.push(verification(ctx, TrunkSeq(seq)).await?);
            }
        }
        Ok(pb::BisectResponse {
            culprit: Some(culprit.get()),
            tested,
        })
    }

    /// Open the repository a trunk position name is in, and the position,
    /// which must have landed.
    pub(super) async fn landing<T>(
        &self,
        request: &Call<T>,
        name: &str,
    ) -> Result<(Opened, TrunkSeq)> {
        let opened = open_repo(&self.state, request, name).await?;
        let seq = names::position_in(&opened.repo, name)?;
        if opened.repo.repo.trunk_at(seq).await?.is_none() {
            return Err(Error::not_found(format!("nothing landed at {seq}")));
        }
        Ok((opened, seq))
    }
}
