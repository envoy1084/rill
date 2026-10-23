//! Human attention: the inbox, sampled audits, batch review and intent
//! digests. Results are the wire records.

use rill_proto::rill::v1 as pb;
use rill_types::{IntentId, OperationId, TrunkSeq};

use crate::client::Client;
use crate::error::Result;
use crate::names;

/// Attention operations on one repository.
#[derive(Clone, Debug)]
pub struct AttentionClient {
    client: Client,
    repo: String,
}

impl AttentionClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        AttentionClient { client, repo }
    }

    fn svc(&self) -> crate::client::AttentionSvc {
        self.client.attention_svc()
    }

    fn landing(&self, seq: TrunkSeq) -> String {
        format!("{}/trunk/positions/{}", self.repo, seq.0)
    }

    /// What waits on the caller, most urgent first.
    pub async fn inbox(&self, limit: u32) -> Result<Vec<pb::InboxItem>> {
        let request = pb::ListInboxRequest {
            parent: self.repo.clone(),
            limit,
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_inbox(request).await }
        })
        .await?
        .into_owned()
        .items)
    }

    /// Audits still waiting.
    pub async fn audits(&self) -> Result<Vec<pb::Audit>> {
        let request = pb::ListAuditsRequest {
            parent: self.repo.clone(),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_audits(request).await }
        })
        .await?
        .into_owned()
        .audits)
    }

    /// Pass or fail the audit of the landing at `seq`; a failure can also
    /// revert it.
    pub async fn record_audit(
        &self,
        seq: TrunkSeq,
        passed: bool,
        note: &str,
        revert: bool,
    ) -> Result<pb::Audit> {
        let request = pb::RecordAuditRequest {
            request_id: names::request_id(&OperationId::new()),
            landing: self.landing(seq),
            passed,
            note: note.to_string(),
            revert,
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.record_audit(request).await }
        })
        .await?
        .into_owned();
        Ok(rill_proto::convert::required(
            "record_audit.audit",
            resp.audit,
        )?)
    }

    /// The intent's changes in review, grouped by the edit they make.
    pub async fn similar(&self, intent: IntentId) -> Result<Vec<pb::SimilarGroup>> {
        let request = pb::ListSimilarChangesRequest {
            intent: names::intent(&self.repo, &intent),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_similar_changes(request).await }
        })
        .await?
        .into_owned()
        .groups)
    }

    /// Vote on every change of the group `signature` under `intent`.
    pub async fn batch_vote(
        &self,
        intent: IntentId,
        signature: &str,
        label: &str,
        value: i8,
        reason: &str,
    ) -> Result<Vec<pb::BatchVoteResult>> {
        let request = pb::BatchVoteRequest {
            request_id: names::request_id(&OperationId::new()),
            intent: names::intent(&self.repo, &intent),
            signature: signature.to_string(),
            label: label.to_string(),
            value: value.into(),
            reason: reason.to_string(),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.batch_vote(request).await }
        })
        .await?
        .into_owned()
        .results)
    }

    /// What happened under `intent` on `day` (`YYYY-MM-DD`; today when
    /// empty).
    pub async fn digest(&self, intent: IntentId, day: &str) -> Result<pb::GetIntentDigestResponse> {
        let request = pb::GetIntentDigestRequest {
            intent: names::intent(&self.repo, &intent),
            day: day.to_string(),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.get_intent_digest(request).await }
        })
        .await?
        .into_owned())
    }
}
