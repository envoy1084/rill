//! The reviewer fleet: leasing review and resolve jobs as an agent,
//! reporting results, and asking for and listing reviews.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeVersionHash, OperationId};

use crate::client::Client;
use crate::error::Result;
use crate::names;
use crate::types::{Lease, ResolveJob, ReviewJob, ReviewReport, ReviewRun, ReviewerStats};
use rill_proto::convert::timestamp_from_proto;

/// Reviewer fleet operations on one repository.
#[derive(Clone, Debug)]
pub struct ReviewerClient {
    client: Client,
    repo: String,
}

fn lease(token: bytes::Bytes, until: i64) -> Option<Lease> {
    (!token.is_empty()).then(|| Lease {
        token: token.to_vec(),
        until: timestamp_from_proto(until),
    })
}

impl ReviewerClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        ReviewerClient { client, repo }
    }

    fn svc(&self) -> crate::client::ReviewerSvc {
        self.client.reviewer_svc()
    }

    /// Lease the calling reviewer's next job, waiting up to `wait_ms` for
    /// one.
    pub async fn lease_review(&self, wait_ms: u32) -> Result<Option<(Lease, ReviewJob)>> {
        let request = pb::LeaseReviewJobRequest {
            parent: self.repo.clone(),
            wait_ms,
        };
        let resp = self.svc().lease_review_job(request).await?.into_owned();
        let Some(held) = lease(resp.lease, resp.lease_until) else {
            return Ok(None);
        };
        let job = rill_proto::convert::required("lease_review_job.job", resp.job)?;
        Ok(Some((held, job.try_into()?)))
    }

    /// Extend a lease.
    pub async fn renew(&self, held: &Lease) -> Result<Lease> {
        let request = pb::RenewReviewLeaseRequest {
            parent: self.repo.clone(),
            lease: held.token.clone().into(),
        };
        let resp = self.svc().renew_review_lease(request).await?.into_owned();
        lease(resp.lease, resp.lease_until)
            .ok_or_else(|| crate::Error::config("the server returned no lease"))
    }

    /// Report a review.
    pub async fn complete_review(&self, held: &Lease, report: &ReviewReport) -> Result<ReviewRun> {
        let request = pb::CompleteReviewJobRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            lease: held.token.clone().into(),
            findings: report
                .findings
                .iter()
                .map(pb::ReviewFinding::from)
                .collect(),
            vote: report.vote.into(),
            summary: report.summary.clone(),
            model: report.model.clone(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.complete_review_job(request).await }
        })
        .await?
        .into_owned();
        rill_proto::convert::required("complete_review_job.run", resp.run)?.try_into()
    }

    /// Give a job up: to retry later, or for good.
    pub async fn fail(&self, held: &Lease, error: &str, retry: bool) -> Result<()> {
        let request = pb::FailReviewJobRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            lease: held.token.clone().into(),
            error: error.to_string(),
            retry,
        };
        crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.fail_review_job(request).await }
        })
        .await?;
        Ok(())
    }

    /// Queue reviews of the change's current version: by `reviewers`, or
    /// as trunk's `.rill/review.toml` routes it when empty.
    pub async fn request(&self, change: ChangeId, reviewers: &[String]) -> Result<Vec<ReviewRun>> {
        let request = pb::RequestReviewRequest {
            request_id: names::request_id(&OperationId::new()),
            change: names::change(&self.repo, &change),
            reviewers: reviewers.to_vec(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.request_review(request).await }
        })
        .await?
        .into_owned();
        resp.runs.into_iter().map(ReviewRun::try_from).collect()
    }

    /// The change's runs, newest first.
    pub async fn runs(&self, change: ChangeId) -> Result<Vec<ReviewRun>> {
        let request = pb::ListReviewRunsRequest {
            change: names::change(&self.repo, &change),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_review_runs(request).await }
        })
        .await?
        .into_owned();
        resp.runs.into_iter().map(ReviewRun::try_from).collect()
    }

    /// Lease the calling resolver's next conflict.
    pub async fn lease_resolve(&self, wait_ms: u32) -> Result<Option<(Lease, ResolveJob)>> {
        let request = pb::LeaseResolveJobRequest {
            parent: self.repo.clone(),
            wait_ms,
        };
        let resp = self.svc().lease_resolve_job(request).await?.into_owned();
        let Some(held) = lease(resp.lease, resp.lease_until) else {
            return Ok(None);
        };
        let job = rill_proto::convert::required("lease_resolve_job.job", resp.job)?;
        Ok(Some((held, job.try_into()?)))
    }

    /// Report a resolve job: the version stored, or `None` for declined.
    pub async fn complete_resolve(
        &self,
        held: &Lease,
        resolved: Option<ChangeVersionHash>,
        rationale: &str,
    ) -> Result<ReviewRun> {
        let request = pb::CompleteResolveJobRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            lease: held.token.clone().into(),
            resolved_version: resolved
                .map(|v| names::version(&self.repo, &v))
                .unwrap_or_default(),
            rationale: rationale.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.complete_resolve_job(request).await }
        })
        .await?
        .into_owned();
        rill_proto::convert::required("complete_resolve_job.run", resp.run)?.try_into()
    }

    /// Reviewers' track records in the repository.
    pub async fn stats(&self) -> Result<Vec<ReviewerStats>> {
        let request = pb::ListReviewerStatsRequest {
            parent: self.repo.clone(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_reviewer_stats(request).await }
        })
        .await?
        .into_owned();
        Ok(resp
            .reviewers
            .into_iter()
            .map(ReviewerStats::from)
            .collect())
    }

    /// Clear a reviewer's record and lift its demotion.
    pub async fn reinstate(&self, reviewer: &str) -> Result<ReviewerStats> {
        let request = pb::ReinstateReviewerRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.repo.clone(),
            reviewer: reviewer.to_string(),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.reinstate_reviewer(request).await }
        })
        .await?
        .into_owned();
        Ok(rill_proto::convert::required("reinstate_reviewer.stats", resp.stats)?.into())
    }
}
