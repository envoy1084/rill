//! The effective submit policy, why a landing was allowed, and replaying
//! history under a candidate policy.

use rill_policy::submit::simulate::replay;
use rill_policy::SubmitPolicy;
use rill_proto::rill::v1 as pb;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::{decisions, policy};
use rill_types::{RepoPathBuf, TrunkSeq};

use super::PolicySvc;
use crate::services::common::open_repo;
use crate::services::{names, Call};

/// Most landings one simulation replays.
const MAX_REPLAY: u64 = 10_000;

impl PolicySvc {
    pub(super) async fn get_effective_policy_impl(
        &self,
        request: Call<pb::GetEffectivePolicyRequest>,
    ) -> Result<pb::GetEffectivePolicyResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let review = opened.repo.review();
        let digest = review.policy().digest();
        let text = policy::policy_text(&opened.repo.repo, &digest)
            .await?
            .unwrap_or_default();
        Ok(pb::GetEffectivePolicyResponse {
            text,
            source: review.policy_source().to_string(),
            digest: hex::encode(digest),
        })
    }

    pub(super) async fn explain_landing_impl(
        &self,
        request: Call<pb::ExplainLandingRequest>,
    ) -> Result<pb::ExplainLandingResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.landing).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let seq = names::position_in(&opened.repo, &req.landing)?;
        let decision = decisions::load(&opened.repo.repo, seq)
            .await?
            .ok_or_else(|| {
                Error::not_found(format!(
                "no decision is recorded for trunk@{} (landed without review, or not yet recorded)",
                seq.0
            ))
            })?;
        let text = match digest_of(&decision.policy) {
            Some(digest) => policy::policy_text(&opened.repo.repo, &digest).await?,
            None => None,
        };
        let facts = serde_json::json!({
            "change": decision.record.change,
            "review": decision.record.review,
        });
        Ok(pb::ExplainLandingResponse {
            change: names::change(&opened.repo, &decision.change),
            policy: decision.policy,
            source: decision.source,
            allowed: decision.record.decision.allowed,
            matched_policies: decision.record.decision.matched_policies,
            reasons: decision.record.decision.reasons,
            facts_json: serde_json::to_string_pretty(&facts)
                .map_err(|e| Error::internal(e.to_string()))?,
            policy_text: text.unwrap_or_default(),
        })
    }

    pub(super) async fn simulate_policy_impl(
        &self,
        request: Call<pb::SimulatePolicyRequest>,
    ) -> Result<pb::SimulatePolicyResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let candidate = SubmitPolicy::from_source(&req.text)?;
        let head = opened
            .repo
            .repo
            .trunk_head()
            .await?
            .map_or(0, |(seq, _)| seq.0);
        let since = req.since.max(1);
        let until = if req.until == 0 {
            head
        } else {
            req.until.min(head)
        };
        if until >= since && until - since >= MAX_REPLAY {
            return Err(Error::invalid(format!(
                "replay at most {MAX_REPLAY} landings at a time"
            )));
        }
        let mut out = pb::SimulatePolicyResponse::default();
        for n in since..=until {
            let Some(d) = decisions::load(&opened.repo.repo, TrunkSeq(n)).await? else {
                out.unrecorded += 1;
                continue;
            };
            out.replayed += 1;
            let before_policy = recorded_policy(&opened.repo.repo, &d).await?;
            if let Some(p) = &before_policy {
                if rill_policy::submit::simulate::agent_only(p, &d.record)? {
                    out.agent_only_before += 1;
                }
            }
            let r = replay(&candidate, &d.record)?;
            if r.agent_only_after {
                out.agent_only_after += 1;
            }
            if r.after.allowed {
                out.allowed += 1;
            } else if d.record.decision.allowed {
                out.blocked.push(pb::SimulatedLanding {
                    landing: names::trunk_position(&opened.repo, TrunkSeq(n)),
                    change: names::change(&opened.repo, &d.change),
                    reasons: r.after.reasons,
                });
            }
        }
        Ok(out)
    }
}

/// The policy a recorded decision was made under, when its text was kept.
async fn recorded_policy(
    repo: &rill_store::RepoStore,
    d: &decisions::LandingDecision,
) -> Result<Option<SubmitPolicy>> {
    let Some(digest) = digest_of(&d.policy) else {
        return Ok(None);
    };
    match policy::policy_text(repo, &digest).await? {
        Some(text) => Ok(Some(SubmitPolicy::from_source(&text)?)),
        None => Ok(None),
    }
}

fn digest_of(hex_digest: &str) -> Option<[u8; 32]> {
    hex::decode(hex_digest).ok()?.try_into().ok()
}
