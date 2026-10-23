//! Approving an intent's scope, withdrawing an approval, and explaining
//! whether a change inherits its intent's approval.

use rill_policy::intents::{missing_approvals, scope_dirs};
use rill_policy::owners::{counts_for, Approver, OWNERS_FILE};
use rill_policy::{Relation, Verb};
use rill_proto::rill::v1 as pb;
use rill_types::{Intent, IntentApproval, RepoPathBuf, Timestamp};

use super::records::{intent_proto, load};
use super::IntentSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

/// Read, change and write back an intent, retrying when another write
/// came first.
async fn update(
    opened: &Opened,
    name: &str,
    mut f: impl FnMut(&mut Intent) -> Result<()>,
) -> Result<Intent> {
    for _ in 0..rill_review::lifecycle::MAX_WRITE_ATTEMPTS {
        let current = load(opened, name).await?;
        let mut next = current.clone();
        f(&mut next)?;
        next.updated_at = Timestamp::now();
        match opened
            .repo
            .repo
            .put_intent_if(&next, &current, opened.principal())
            .await
        {
            Ok(()) => return Ok(next),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(rill_store::Error::Conflict("the intent kept changing; retry".into()).into())
}

/// The scope directories `opened`'s caller owns: by an owner set covering
/// the directory, or by the `owner` relation where no `OWNERS.toml` does.
async fn owned_dirs(opened: &Opened, intent: &Intent) -> Result<Vec<String>> {
    let Some(scope) = &intent.scope else {
        return Err(Error::precondition(format!(
            "intent {} has no scope to approve",
            intent.id
        )));
    };
    let dirs = scope_dirs(scope);
    if opened.dev_mode() {
        return Ok(dirs.into_iter().map(|d| d.into_string()).collect());
    }
    let owners = opened.repo.review().trunk_owners().await?;
    let me = Approver {
        principal: opened.principal().clone(),
        teams: opened.ctx.teams.clone(),
    };
    let now = Timestamp::now();
    let mut owned = Vec::new();
    for dir in dirs {
        let probe = dir.join(OWNERS_FILE)?;
        let sets = owners.sets_for(&probe);
        let owns = if sets.is_empty() {
            opened
                .permit()
                .check(Verb::Review, Relation::Owner, &dir)
                .await
                .is_ok()
        } else {
            sets.iter().any(|s| counts_for(s, &me, now))
        };
        if owns {
            owned.push(dir.into_string());
        }
    }
    Ok(owned)
}

impl IntentSvc {
    pub(super) async fn approve_intent_impl(
        &self,
        request: Call<pb::ApproveIntentRequest>,
    ) -> Result<pb::ApproveIntentResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let current = load(&opened, &req.name).await?;
        if req.revision != current.revision {
            return Err(Error::precondition(format!(
                "approving revision {} but the intent is at {}; review the current scope",
                req.revision, current.revision
            )));
        }
        let owns = owned_dirs(&opened, &current).await?;
        if owns.is_empty() {
            return Err(Error::denied(format!(
                "{} owns none of the directories intent {}'s scope covers",
                opened.principal(),
                current.id
            )));
        }
        let me = opened.principal().clone();
        let revision = req.revision;
        let intent = update(&opened, &req.name, |i| {
            if i.revision != revision {
                return Err(Error::precondition("the scope changed while approving"));
            }
            i.approvals.retain(|a| a.by.id != me.id);
            i.approvals.push(IntentApproval {
                by: me.clone(),
                at: Timestamp::now(),
                revision,
                owns: owns.clone(),
            });
            Ok(())
        })
        .await?;
        Ok(pb::ApproveIntentResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }

    pub(super) async fn revoke_intent_approval_impl(
        &self,
        request: Call<pb::RevokeIntentApprovalRequest>,
    ) -> Result<pb::RevokeIntentApprovalResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let me = opened.principal().id;
        let intent = update(&opened, &req.name, |i| {
            i.approvals.retain(|a| a.by.id != me);
            Ok(())
        })
        .await?;
        Ok(pb::RevokeIntentApprovalResponse {
            intent: intent_proto(&opened.repo, intent).into(),
        })
    }

    pub(super) async fn explain_intent_scope_impl(
        &self,
        request: Call<pb::ExplainIntentScopeRequest>,
    ) -> Result<pb::ExplainIntentScopeResponse> {
        let name = &request.get_ref().change;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let change = names::change_in(&opened.repo, name)?;
        let review = opened.repo.review();
        let state = review.get(&change).await?;
        let facts = review.change_facts(&state).await?;
        let Some(intent_facts) = facts.intent else {
            return Ok(pb::ExplainIntentScopeResponse::default());
        };
        let version = state
            .current_version()
            .ok_or_else(|| Error::precondition(format!("{change} has no version")))?;
        let id = opened
            .repo
            .derive
            .change(&version)
            .await?
            .intent
            .or(state.intent)
            .ok_or_else(|| Error::internal("intent facts without an intent"))?;
        let intent = opened
            .repo
            .repo
            .get_intent(&id)
            .await?
            .ok_or_else(|| Error::not_found(format!("intent {id}")))?;
        let owners = review.trunk_owners().await?;
        Ok(pb::ExplainIntentScopeResponse {
            intent: names::intent_name(&opened.repo, &id),
            approved: intent_facts.approved,
            in_scope: intent_facts.in_scope,
            reasons: intent_facts.reasons,
            missing_approvals: missing_approvals(&intent, &owners).unwrap_or_default(),
        })
    }
}
