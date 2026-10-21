//! Lander: trunk status, landings, verification, reverts and bisects.

use std::collections::BTreeMap;

use rill_proto::convert::{hash_to_proto, id_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, TrunkSeq};

use crate::client::Client;
use crate::error::Result;
use crate::types::{
    BisectResult, LandEvent, Landing, NewVerification, RevertResult, TrunkStatus,
    VerificationRecord,
};

/// Lander operations on one repository.
#[derive(Clone, Debug)]
pub struct LanderClient {
    client: Client,
    repo: String,
}

impl LanderClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        LanderClient { client, repo }
    }

    fn svc(&self) -> crate::client::LanderSvc {
        self.client.lander_svc()
    }

    /// Trunk head, in-flight landings and the `recent_limit` latest
    /// landings.
    pub async fn trunk_status(&self, recent_limit: u32) -> Result<TrunkStatus> {
        self.svc()
            .get_trunk_status(pb::GetTrunkStatusRequest {
                repo: self.repo.clone(),
                recent_limit,
            })
            .await?
            .into_inner()
            .try_into()
    }

    /// Landings before `before_seq` (the head when `None`), newest first,
    /// each paired with its verification record.
    pub async fn landings(&self, limit: u32, before_seq: Option<TrunkSeq>) -> Result<Vec<Landing>> {
        let resp = self
            .svc()
            .list_landings(pb::ListLandingsRequest {
                repo: self.repo.clone(),
                limit,
                before_seq: before_seq.map(TrunkSeq::get),
            })
            .await?
            .into_inner();
        let mut verifications: BTreeMap<TrunkSeq, VerificationRecord> = resp
            .verifications
            .into_iter()
            .map(|v| VerificationRecord::try_from(v).map(|r| (r.seq, r)))
            .collect::<Result<_>>()?;
        resp.landings
            .into_iter()
            .map(|l| {
                let event = LandEvent::try_from(l)?;
                let verification = verifications.remove(&event.seq);
                Ok(Landing {
                    event,
                    verification,
                })
            })
            .collect()
    }

    /// Verification state of one landed sequence.
    pub async fn verification(&self, seq: TrunkSeq) -> Result<VerificationRecord> {
        let resp = self
            .svc()
            .get_verification(pb::GetVerificationRequest {
                repo: self.repo.clone(),
                seq: seq.get(),
            })
            .await?
            .into_inner();
        required("get_verification.result", resp.result)?.try_into()
    }

    /// Report a verification outcome for a landed sequence.
    pub async fn report_verification(
        &self,
        seq: TrunkSeq,
        report: NewVerification,
    ) -> Result<VerificationRecord> {
        self.report_verification_with_op(OperationId::new(), seq, report)
            .await
    }

    /// [`LanderClient::report_verification`] with a caller-chosen
    /// idempotency key.
    pub async fn report_verification_with_op(
        &self,
        op: OperationId,
        seq: TrunkSeq,
        report: NewVerification,
    ) -> Result<VerificationRecord> {
        let request = pb::ReportVerificationRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            seq: seq.get(),
            target: report.target,
            status: report.status.to_proto(),
            details: report.details,
            artifact: report.artifact.as_ref().map(hash_to_proto),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.report_verification(request).await }
        })
        .await?
        .into_inner();
        required("report_verification.result", resp.result)?.try_into()
    }

    /// Land a revert of a sequence.
    pub async fn revert(&self, seq: TrunkSeq, reason: &str) -> Result<RevertResult> {
        self.revert_with_op(OperationId::new(), seq, reason).await
    }

    /// [`LanderClient::revert`] with a caller-chosen idempotency key.
    pub async fn revert_with_op(
        &self,
        op: OperationId,
        seq: TrunkSeq,
        reason: &str,
    ) -> Result<RevertResult> {
        let request = pb::RevertSeqRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            seq: seq.get(),
            reason: reason.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.revert_seq(request).await }
        })
        .await?
        .into_inner();
        Ok(RevertResult {
            landing: required("revert_seq.landing", resp.landing)?.try_into()?,
            version: required("revert_seq.version", resp.version)?.try_into()?,
        })
    }

    /// Bisect a verification target between a known-good and a known-bad
    /// sequence.
    pub async fn bisect(&self, from: TrunkSeq, to: TrunkSeq, target: &str) -> Result<BisectResult> {
        self.bisect_with_op(OperationId::new(), from, to, target)
            .await
    }

    /// [`LanderClient::bisect`] with a caller-chosen idempotency key.
    pub async fn bisect_with_op(
        &self,
        op: OperationId,
        from: TrunkSeq,
        to: TrunkSeq,
        target: &str,
    ) -> Result<BisectResult> {
        let request = pb::BisectRequest {
            operation_id: id_to_proto(&op),
            repo: self.repo.clone(),
            from: from.get(),
            to: to.get(),
            target: target.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (mut svc, request) = (self.svc(), request.clone());
            async move { svc.bisect(request).await }
        })
        .await?
        .into_inner();
        Ok(BisectResult {
            culprit: resp.culprit.map(TrunkSeq),
            tested: resp
                .tested
                .into_iter()
                .map(VerificationRecord::try_from)
                .collect::<Result<_>>()?,
        })
    }
}
