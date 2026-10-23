//! Batch review and intent digests.

use rill_proto::rill::v1 as pb;
use rill_runtime::error::{Error, Result};
use rill_types::{Intent, RepoPathBuf, Timestamp};

use super::AttentionSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};

async fn intent(opened: &Opened, name: &str) -> Result<Intent> {
    let id = names::intent_in(&opened.repo, name)?;
    opened
        .repo
        .repo
        .get_intent(&id)
        .await?
        .ok_or_else(|| Error::not_found(format!("intent {id}")))
}

impl AttentionSvc {
    pub(super) async fn list_similar_changes_impl(
        &self,
        request: Call<pb::ListSimilarChangesRequest>,
    ) -> Result<pb::ListSimilarChangesResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.intent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let intent = intent(&opened, &req.intent).await?;
        let groups = opened.repo.review().similar_changes(&intent).await?;
        let ctx = &opened.repo;
        Ok(pb::ListSimilarChangesResponse {
            groups: groups
                .into_iter()
                .map(|g| pb::SimilarGroup {
                    signature: g.signature,
                    changes: g
                        .changes
                        .iter()
                        .map(|(c, _)| names::change(ctx, c))
                        .collect(),
                    versions: g
                        .changes
                        .iter()
                        .map(|(_, v)| names::version(ctx, v))
                        .collect(),
                })
                .collect(),
        })
    }

    pub(super) async fn batch_vote_impl(
        &self,
        request: Call<pb::BatchVoteRequest>,
    ) -> Result<pb::BatchVoteResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.intent).await?;
        opened.permit().review().await?;
        let value = i8::try_from(req.value).map_err(|_| Error::invalid("vote out of range"))?;
        if req.label == rill_policy::submit::OWNER_LABEL && value > 0 {
            return Err(Error::invalid(
                "owner approvals are given change by change, not in a batch",
            ));
        }
        let intent = intent(&opened, &req.intent).await?;
        let review = opened.repo.review();
        let group = review
            .similar_changes(&intent)
            .await?
            .into_iter()
            .find(|g| g.signature == req.signature)
            .ok_or_else(|| {
                Error::precondition(format!(
                    "no changes of intent {} make the edit {}; list them again",
                    intent.id, req.signature
                ))
            })?;
        let reason = (!req.reason.is_empty()).then(|| req.reason.clone());
        let mut results = Vec::with_capacity(group.changes.len());
        for (change, version) in group.changes {
            let voted = review
                .vote_at(
                    &change,
                    opened.principal(),
                    &req.label,
                    value,
                    reason.clone(),
                    Some(version),
                )
                .await;
            results.push(pb::BatchVoteResult {
                change: names::change(&opened.repo, &change),
                voted: voted.is_ok(),
                error: voted.err().map(|e| e.to_string()).unwrap_or_default(),
            });
        }
        Ok(pb::BatchVoteResponse { results })
    }

    pub(super) async fn get_intent_digest_impl(
        &self,
        request: Call<pb::GetIntentDigestRequest>,
    ) -> Result<pb::GetIntentDigestResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.intent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let intent = intent(&opened, &req.intent).await?;
        let day = if req.day.is_empty() {
            rill_policy::intents::day_of(Timestamp::now())
        } else {
            req.day.clone()
        };
        let d = opened.repo.review().intent_digest(&intent, &day).await?;
        Ok(pb::GetIntentDigestResponse {
            day: d.day,
            landed: d.landed,
            reverted: d.reverted,
            flagged: d.flagged,
            sampled: d.sampled,
            open: d.open,
        })
    }
}
