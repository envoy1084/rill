//! Trunk: status, landings, verification, healing, reverts and bisects.

use std::collections::BTreeMap;

use rill_proto::convert::{hash_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, TrunkSeq};

use crate::client::Client;
use crate::error::Result;
use crate::names;
use crate::types::{
    BisectResult, LandEvent, Landing, NewVerification, Page, RevertResult, RevertSelector,
    TrunkStatus, VerificationRecord,
};

/// Trunk operations on one repository.
#[derive(Clone, Debug)]
pub struct TrunkClient {
    client: Client,
    /// The repository's full resource name.
    repo: String,
}

impl TrunkClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        TrunkClient { client, repo }
    }

    fn svc(&self) -> crate::client::TrunkSvc {
        self.client.trunk_svc()
    }

    fn landing(&self, seq: TrunkSeq) -> String {
        format!("{}/trunk/positions/{}", self.repo, seq.get())
    }

    /// Trunk head, in-flight landings and the `recent_limit` latest
    /// landings.
    pub async fn trunk_status(&self, recent_limit: u32) -> Result<TrunkStatus> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetTrunkStatusRequest {
                parent: self.repo.clone(),
            };
            async move { svc.get_trunk_status(request).await }
        })
        .await?
        .into_owned();
        let recent = match recent_limit {
            0 => Vec::new(),
            n => self
                .landings_page(n, None)
                .await?
                .items
                .into_iter()
                .map(|l| l.event)
                .collect(),
        };
        TrunkStatus::from_parts(resp, recent)
    }

    /// Up to `limit` landings, newest first, each paired with its
    /// verification record.
    pub async fn landings(&self, limit: u32) -> Result<Vec<Landing>> {
        Ok(self.landings_page(limit, None).await?.items)
    }

    /// One page of landings, newest first; pass the returned token to
    /// continue.
    pub async fn landings_page(
        &self,
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<Page<Landing>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListLandingsRequest {
                parent: self.repo.clone(),
                page_size: page_size as i32,
                page_token: page_token.unwrap_or_default().to_string(),
            };
            async move { svc.list_landings(request).await }
        })
        .await?
        .into_owned();
        let mut verifications: BTreeMap<TrunkSeq, VerificationRecord> = resp
            .verifications
            .into_iter()
            .map(|v| VerificationRecord::try_from(v).map(|r| (r.seq, r)))
            .collect::<Result<_>>()?;
        let items = resp
            .landings
            .into_iter()
            .map(|l| {
                let event = LandEvent::try_from(l)?;
                let verification = verifications.remove(&event.seq);
                Ok(Landing {
                    event,
                    verification,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Page::new(
            items,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
    }

    /// Verification state of one landed position.
    pub async fn verification(&self, seq: TrunkSeq) -> Result<VerificationRecord> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetVerificationRequest {
                landing: self.landing(seq),
            };
            async move { svc.get_verification(request).await }
        })
        .await?
        .into_owned();
        required("get_verification.verification", resp.verification)?.try_into()
    }

    /// Report a verification outcome for a landed position.
    pub async fn report_verification(
        &self,
        seq: TrunkSeq,
        report: NewVerification,
    ) -> Result<VerificationRecord> {
        self.report_verification_with_op(OperationId::new(), seq, report)
            .await
    }

    /// [`TrunkClient::report_verification`] with a caller-chosen
    /// idempotency key.
    pub async fn report_verification_with_op(
        &self,
        op: OperationId,
        seq: TrunkSeq,
        report: NewVerification,
    ) -> Result<VerificationRecord> {
        let request = pb::ReportVerificationRequest {
            request_id: names::request_id(&op),
            landing: self.landing(seq),
            target: report.target,
            status: report.status.to_proto().into(),
            details: report.details,
            artifact: report.artifact.as_ref().map(hash_to_proto),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.report_verification(request).await }
        })
        .await?
        .into_owned();
        required("report_verification.verification", resp.verification)?.try_into()
    }

    /// Land a revert of a position.
    pub async fn revert(&self, seq: TrunkSeq, reason: &str) -> Result<RevertResult> {
        self.revert_with_op(OperationId::new(), seq, reason).await
    }

    /// [`TrunkClient::revert`] with a caller-chosen idempotency key.
    pub async fn revert_with_op(
        &self,
        op: OperationId,
        seq: TrunkSeq,
        reason: &str,
    ) -> Result<RevertResult> {
        let request = pb::RevertLandingRequest {
            request_id: names::request_id(&op),
            landing: self.landing(seq),
            reason: reason.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.revert_landing(request).await }
        })
        .await?
        .into_owned();
        Ok(RevertResult {
            landing: required("revert_landing.landing", resp.landing)?.try_into()?,
            version: required("revert_landing.version", resp.version)?.try_into()?,
        })
    }

    /// Bisect a verification target between a known-good and a known-bad
    /// position.
    pub async fn bisect(&self, from: TrunkSeq, to: TrunkSeq, target: &str) -> Result<BisectResult> {
        self.bisect_with_op(OperationId::new(), from, to, target)
            .await
    }

    /// [`TrunkClient::bisect`] with a caller-chosen idempotency key.
    pub async fn bisect_with_op(
        &self,
        op: OperationId,
        from: TrunkSeq,
        to: TrunkSeq,
        target: &str,
    ) -> Result<BisectResult> {
        let request = pb::BisectRequest {
            request_id: names::request_id(&op),
            parent: self.repo.clone(),
            from: from.get(),
            to: to.get(),
            target: target.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.bisect(request).await }
        })
        .await?
        .into_owned();
        Ok(BisectResult {
            culprit: resp.culprit.map(TrunkSeq),
            tested: resp
                .tested
                .into_iter()
                .map(VerificationRecord::try_from)
                .collect::<Result<_>>()?,
        })
    }

    /// The land requests waiting, in serve order, with their place in the
    /// queue and estimated wait.
    pub async fn queue(&self) -> Result<Vec<crate::types::LandProgress>> {
        let requests = crate::paging::all(|page_token| async move {
            let request = pb::ListLandRequestsRequest {
                parent: self.repo.clone(),
                page_size: crate::paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent_landing(&self.client, request).await?;
            Ok((resp.land_requests, resp.next_page_token))
        })
        .await?;
        requests
            .into_iter()
            .map(crate::types::LandProgress::try_from)
            .collect()
    }

    /// Trunk's state and its open incidents.
    pub async fn health(&self) -> Result<(pb::TrunkHealth, Vec<pb::Incident>)> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetTrunkHealthRequest {
                parent: self.repo.clone(),
            };
            async move { svc.get_trunk_health(request).await }
        })
        .await?
        .into_owned();
        Ok((required("get_trunk_health.health", resp.health)?, resp.open))
    }

    /// One page of incidents, newest culprit first; `state` keeps `open`,
    /// `fixing` or `closed` ones.
    pub async fn incidents(
        &self,
        state: Option<&str>,
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<Page<pb::Incident>> {
        let filter = crate::filter::FilterBuilder::new()
            .eq_opt("state", state)
            .build();
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListIncidentsRequest {
                parent: self.repo.clone(),
                page_size: page_size as i32,
                page_token: page_token.unwrap_or_default().to_string(),
                filter: filter.clone(),
            };
            async move { svc.list_incidents(request).await }
        })
        .await?
        .into_owned();
        Ok(Page::new(
            resp.incidents,
            Some(resp.next_page_token).filter(|t| !t.is_empty()),
        ))
    }

    /// Freeze trunk so only emergency landings land, or thaw it.
    pub async fn set_frozen(&self, frozen: bool, reason: &str) -> Result<pb::TrunkHealth> {
        let request_id = names::request_id(&OperationId::new());
        let health = match frozen {
            true => {
                let request = pb::FreezeTrunkRequest {
                    request_id,
                    parent: self.repo.clone(),
                    reason: reason.to_string(),
                };
                crate::client::retry_idempotent(|| {
                    let (svc, request) = (self.svc(), request.clone());
                    async move { svc.freeze_trunk(request).await }
                })
                .await?
                .into_owned()
                .health
            }
            false => {
                let request = pb::UnfreezeTrunkRequest {
                    request_id,
                    parent: self.repo.clone(),
                };
                crate::client::retry_idempotent(|| {
                    let (svc, request) = (self.svc(), request.clone());
                    async move { svc.unfreeze_trunk(request).await }
                })
                .await?
                .into_owned()
                .health
            }
        };
        Ok(required("trunk.health", health)?)
    }

    /// What reverting everything `selector` covers would revert, newest
    /// first, with the paths each revert would conflict at.
    pub async fn preview_bulk_revert(
        &self,
        selector: &RevertSelector,
    ) -> Result<Vec<pb::BulkRevertEntry>> {
        let request = pb::PreviewBulkRevertRequest {
            parent: self.repo.clone(),
            selector: selector.to_proto(&self.repo).into(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.preview_bulk_revert(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.entries)
    }

    /// Queue the reverts of everything `selector` covers in the emergency
    /// lane; the operation's result lists them.
    pub async fn bulk_revert(
        &self,
        selector: &RevertSelector,
        reason: &str,
    ) -> Result<pb::Operation> {
        let request = pb::BulkRevertRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            selector: selector.to_proto(&self.repo).into(),
            reason: reason.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.bulk_revert(request).await }
        })
        .await?
        .into_owned();
        Ok(required("bulk_revert.operation", resp.operation)?)
    }
}

async fn retry_idempotent_landing(
    client: &Client,
    request: pb::ListLandRequestsRequest,
) -> Result<pb::ListLandRequestsResponse> {
    Ok(crate::client::retry_idempotent(|| {
        let (svc, request) = (client.landing_svc(), request.clone());
        async move { svc.list_land_requests(request).await }
    })
    .await?
    .into_owned())
}
