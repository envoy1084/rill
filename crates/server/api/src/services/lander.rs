//! `rill.v1.LanderService`: trunk status, verification, reverts and
//! bisection.
//!
//! Verification results come from two sources: the post-land verifier's
//! own records (`rill_lander::postland`) and reports external runners file
//! through `ReportVerification`, kept per sequence in the misc keyspace.

use std::sync::Arc;

use bytes::Bytes;
use rill_lander::postland::{record_at, VerificationRecord, VerificationStatus as PostStatus};
use rill_proto::convert::{hash_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::lander_service_server::LanderService;
use rill_types::{Principal, RepoPathBuf, Timestamp, TrunkSeq};
use serde::{Deserialize, Serialize};
use tonic::{Request, Response, Status};

use super::common::{self, open, GrpcResult};
use super::idem;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::{AppState, RepoContext};

/// Default number of recent landings in a status response.
const DEFAULT_RECENT: usize = 20;
const MAX_LIST: usize = 1000;

/// The lander service.
#[derive(Clone)]
pub struct LanderSvc {
    state: Arc<AppState>,
}

impl LanderSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        LanderSvc { state }
    }
}

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
async fn verification_result(ctx: &RepoContext, seq: TrunkSeq) -> Result<pb::VerificationResult> {
    let record = record_at(&ctx.repo, seq).await?;
    let external = load_reports(ctx, seq).await?;
    let mut reports: Vec<pb::VerificationReport> = Vec::with_capacity(external.len() + 1);
    if let Some(r) = &record {
        reports.push(pb::VerificationReport {
            target: "postland".into(),
            status: post_status_proto(r) as i32,
            reporter: Some(pb::Principal::from(rill_lander::revert::lander_principal())),
            reported_at: timestamp_to_proto(r.at),
            details: Some(r.details.clone()),
            artifact: None,
        });
    }
    for r in &external {
        reports.push(pb::VerificationReport {
            target: r.target.clone(),
            status: r.status,
            reporter: Some(pb::Principal::from(&r.reporter)),
            reported_at: timestamp_to_proto(r.reported_at),
            details: r.details.clone(),
            artifact: r.artifact.as_ref().map(|a| Bytes::copy_from_slice(a)),
        });
    }
    let status = aggregate(reports.iter().map(|r| r.status));
    Ok(pb::VerificationResult {
        seq: seq.get(),
        status: status as i32,
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
        let s = S::try_from(status).unwrap_or(S::Unspecified);
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

#[tonic::async_trait]
impl LanderService for LanderSvc {
    async fn get_trunk_status(
        &self,
        request: Request<pb::GetTrunkStatusRequest>,
    ) -> GrpcResult<pb::GetTrunkStatusResponse> {
        let result: rill_runtime::error::Result<Response<pb::GetTrunkStatusResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let ctx = &opened.repo;
            let head = ctx.repo.trunk_head().await?;
            let (head_seq, head_hash) = match head {
                Some((s, h)) => (s.get(), hash_to_proto(&h)),
                None => (0, Bytes::new()),
            };
            let mut in_flight = Vec::new();
            for queued in ctx.lander.queue().pending(MAX_LIST).await? {
                in_flight.push(pb::InFlightLanding {
                    ticket: Some(common::ticket_proto(&queued.record())),
                    version: hash_to_proto(&queued.request.version),
                });
            }
            let limit = common::limit(request.get_ref().recent_limit, DEFAULT_RECENT, MAX_LIST);
            let landings = ctx.lander.list_landings().await?;
            let mut recent = Vec::new();
            for landing in landings.iter().rev().take(limit) {
                recent.push(common::land_event(ctx, landing.seq, landing.version).await?);
            }
            let last_verified_seq = match &ctx.postland {
                Some(postland) => postland
                    .recent()
                    .await?
                    .iter()
                    .filter(|r| r.is_green())
                    .map(|r| r.seq.get())
                    .max(),
                None => None,
            };
            Ok(Response::new(pb::GetTrunkStatusResponse {
                head_seq,
                head_hash,
                in_flight,
                recent,
                last_verified_seq,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn list_landings(
        &self,
        request: Request<pb::ListLandingsRequest>,
    ) -> GrpcResult<pb::ListLandingsResponse> {
        let result: rill_runtime::error::Result<Response<pb::ListLandingsResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let req = request.get_ref();
            let ctx = &opened.repo;
            let limit = common::limit(req.limit, 50, MAX_LIST);
            let Some((head, _)) = ctx.repo.trunk_head().await? else {
                return Ok(Response::new(pb::ListLandingsResponse::default()));
            };
            let to = match req.before_seq {
                Some(b) => TrunkSeq(b.saturating_sub(1)),
                None => head,
            };
            let from = TrunkSeq(to.get().saturating_sub(limit as u64));
            let mut landings = Vec::new();
            let mut verifications = Vec::new();
            for (seq, hash) in ctx.repo.trunk_range(from, to).await?.into_iter().rev() {
                landings.push(common::land_event(ctx, seq, hash).await?);
                verifications.push(verification_result(ctx, seq).await?);
            }
            Ok(Response::new(pb::ListLandingsResponse {
                landings,
                verifications,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn get_verification(
        &self,
        request: Request<pb::GetVerificationRequest>,
    ) -> GrpcResult<pb::GetVerificationResponse> {
        let result: rill_runtime::error::Result<Response<pb::GetVerificationResponse>> = async {
            let opened = open(&self.state, &request, &request.get_ref().repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let seq = TrunkSeq(request.get_ref().seq);
            if opened.repo.repo.trunk_at(seq).await?.is_none() {
                return Err(Error::not_found(format!("nothing landed at {seq}")));
            }
            Ok(Response::new(pb::GetVerificationResponse {
                result: Some(verification_result(&opened.repo, seq).await?),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn report_verification(
        &self,
        request: Request<pb::ReportVerificationRequest>,
    ) -> GrpcResult<pb::ReportVerificationResponse> {
        let result: rill_runtime::error::Result<Response<pb::ReportVerificationResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().land().await?;
                let req = request.get_ref();
                let ctx = &opened.repo;
                let seq = TrunkSeq(req.seq);
                if ctx.repo.trunk_at(seq).await?.is_none() {
                    return Err(Error::not_found(format!("nothing landed at {seq}")));
                }
                if req.target.is_empty() {
                    return Err(Error::invalid("target is required"));
                }
                pb::VerificationStatus::try_from(req.status)
                    .ok()
                    .filter(|s| *s != pb::VerificationStatus::Unspecified)
                    .ok_or_else(|| {
                        Error::invalid(format!("unknown verification status {}", req.status))
                    })?;
                let mut reports = load_reports(ctx, seq).await?;
                // One report per (target, reporter): a re-run replaces its earlier
                // result rather than appending a contradictory one.
                reports.retain(|r| {
                    !(r.target == req.target && r.reporter.id == opened.principal().id)
                });
                reports.push(Report {
                    target: req.target.clone(),
                    status: req.status,
                    reporter: opened.principal().clone(),
                    reported_at: Timestamp::now(),
                    details: req.details.clone(),
                    artifact: req.artifact.as_ref().map(|a| a.to_vec()),
                });
                store_reports(ctx, seq, &reports).await?;
                Ok(Response::new(pb::ReportVerificationResponse {
                    result: Some(verification_result(ctx, seq).await?),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn revert_seq(
        &self,
        request: Request<pb::RevertSeqRequest>,
    ) -> GrpcResult<pb::RevertSeqResponse> {
        let result: rill_runtime::error::Result<Response<pb::RevertSeqResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().land().await?;
                let req = request.get_ref();
                let seq = TrunkSeq(req.seq);
                let reason = if req.reason.is_empty() {
                    "reverted via RevertSeq".to_string()
                } else {
                    req.reason.clone()
                };
                let outcome = rill_lander::revert(
                    &opened.repo.lander,
                    seq,
                    &reason,
                    opened.principal().clone(),
                )
                .await?;
                let landing = common::land_event(&opened.repo, outcome.seq, outcome.hash).await?;
                Ok(Response::new(pb::RevertSeqResponse {
                    landing: Some(landing),
                    version: Some(pb::ChangeVersion::from(outcome.version)),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn bisect(&self, request: Request<pb::BisectRequest>) -> GrpcResult<pb::BisectResponse> {
        let result: rill_runtime::error::Result<Response<pb::BisectResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = open(&self.state, &request, &request.get_ref().repo).await?;
                opened.permit().land().await?;
                let req = request.get_ref();
                let ctx = &opened.repo;
                let postland = ctx.postland.clone().ok_or_else(|| {
                    Error::precondition(
                        "post-land verification is disabled; enable `postland` to bisect",
                    )
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
                        tested.push(verification_result(ctx, TrunkSeq(seq)).await?);
                    }
                }
                Ok(Response::new(pb::BisectResponse {
                    culprit: Some(culprit.get()),
                    tested,
                }))
            })
            .await;
        result.map_err(Status::from)
    }
}
