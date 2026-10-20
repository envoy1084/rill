//! Reverting a landed version.
//!
//! A revert is an ordinary change whose file changes undo the landed
//! version's effect: the tree diff from the landed version's parent to
//! itself, inverted. It is built with the landed version as its parent so
//! that landing it rebases it onto the current trunk head with a proper
//! three-way merge; later changes to the same files surface as conflicts
//! rather than being silently overwritten.

use std::collections::BTreeMap;
use std::sync::Arc;

use rill_core::{diff_trees, RootTree};
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, FileChange, Principal, Provenance, RiskClass,
    Timestamp, TrunkSeq,
};

use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::events::LandEvent;
use crate::land::Lander;
use crate::queue::{LandRequest, LandStatus};

/// Provenance tool name on reverts a human or agent asked for.
pub const REVERT_TOOL: &str = "lander:revert";
/// Provenance tool name on reverts the post-land verifier initiated.
pub const AUTO_REVERT_TOOL: &str = "lander:auto-revert";
/// Provenance key naming the reverted trunk position.
pub const REVERTS_KEY: &str = "reverts";

/// Name of the service principal that authors automatic reverts.
pub const LANDER_SERVICE: &str = "lander";

/// The service principal `svc:lander`.
pub fn lander_principal() -> Principal {
    Principal::service(LANDER_SERVICE)
}

/// A revert that landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertOutcome {
    /// The landed revert version.
    pub version: ChangeVersion,
    pub hash: ChangeVersionHash,
    pub seq: TrunkSeq,
}

/// Build (and store) a version that undoes what landed at `seq`, parented
/// on the landed version itself. It is not landed.
pub async fn build_revert(
    lander: &Lander,
    seq: TrunkSeq,
    reason: &str,
    actor: Principal,
    tool: &str,
) -> Result<ChangeVersion> {
    let repo = lander.repo();
    let derive = lander.derive();
    let landed = repo.trunk_at(seq).await?.ok_or(Error::NotLanded(seq))?;
    let cv = derive.change(&landed).await?;
    let parent_root = match cv.first_parent() {
        Some(parent) => Some(derive.get::<RootTree>(&parent).await?),
        None => None,
    };
    let landed_root = derive.get::<RootTree>(&landed).await?;

    let mut file_changes = BTreeMap::new();
    for entry in diff_trees(derive.trees(), parent_root, Some(landed_root)).await? {
        let change = match entry.before {
            Some((content, kind)) => FileChange::set(content, kind),
            None => FileChange::Delete,
        };
        file_changes.insert(entry.path, change);
    }

    let mut provenance = Provenance {
        tool: Some(tool.to_string()),
        ..Provenance::default()
    };
    provenance
        .extra
        .insert(REVERTS_KEY.to_string(), seq.get().to_string());
    provenance
        .extra
        .insert("reverted_version".to_string(), landed.to_hex());

    let revert = ChangeVersion {
        change_id: ChangeId::new(),
        parents: vec![landed],
        predecessor: None,
        subtree_changes: BTreeMap::new(),
        file_changes,
        author: actor,
        provenance,
        intent: cv.intent,
        message: format!("Revert {seq}: {}\n\n{reason}", cv.summary()),
        created_at: Timestamp::now(),
        extra: BTreeMap::new(),
    };
    repo.put_change_version(&revert).await?;
    Ok(revert)
}

/// Revert what landed at `seq` and land the revert without pre-land
/// verification.
pub async fn revert(
    lander: &Arc<Lander>,
    seq: TrunkSeq,
    reason: &str,
    actor: Principal,
) -> Result<RevertOutcome> {
    revert_with(lander, seq, reason, actor, REVERT_TOOL).await
}

async fn revert_with(
    lander: &Arc<Lander>,
    seq: TrunkSeq,
    reason: &str,
    actor: Principal,
    tool: &str,
) -> Result<RevertOutcome> {
    let version = build_revert(lander, seq, reason, actor.clone(), tool).await?;
    let hash = version.version_hash();
    let mut request = LandRequest::new(version.change_id, hash, actor, RiskClass::Normal);
    request.verify_override = Some(VerifyMode::None);
    match lander.land_now(request).await? {
        LandStatus::Landed { seq: landed_at } => {
            // The pipeline may have rebased; report the version that landed.
            let landed_hash = lander
                .repo()
                .trunk_at(landed_at)
                .await?
                .ok_or(Error::NotLanded(landed_at))?;
            let landed = lander.derive().change(&landed_hash).await?;
            lander.publish(LandEvent::Reverted {
                culprit: seq,
                revert: landed_at,
            });
            Ok(RevertOutcome {
                version: landed,
                hash: landed_hash,
                seq: landed_at,
            })
        }
        other => Err(Error::RevertFailed {
            seq,
            reason: format!("{other:?}"),
        }),
    }
}

/// True when something landed after `seq` that claims to fix or revert
/// it: a version whose message or intent mentions `trunk@<seq>`, or a
/// revert of it.
pub async fn has_forward_fix(lander: &Lander, seq: TrunkSeq) -> Result<bool> {
    let repo = lander.repo();
    let Some((head, _)) = repo.trunk_head().await? else {
        return Ok(false);
    };
    let needle = seq.to_string();
    for (_, hash) in repo.trunk_range(seq, head).await? {
        let cv = lander.derive().change(&hash).await?;
        if cv.provenance.extra.get(REVERTS_KEY) == Some(&seq.get().to_string()) {
            return Ok(true);
        }
        if cv.message.contains(&needle) {
            return Ok(true);
        }
        if let Some(intent) = cv.intent {
            if let Some(intent) = repo.get_intent(&intent).await? {
                if intent.title.contains(&needle) || intent.body.contains(&needle) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Spawn the automatic-revert flow for `culprit`: wait `revert_grace`
/// for a forward fix, then revert as `svc:lander`.
pub fn schedule_auto_revert(lander: Arc<Lander>, culprit: TrunkSeq) {
    let grace = lander.policy().revert_grace;
    tokio::spawn(async move {
        tokio::time::sleep(grace).await;
        match auto_revert(&lander, culprit).await {
            Ok(Some(outcome)) => {
                tracing::warn!(%culprit, revert = %outcome.seq, "auto-reverted culprit")
            }
            Ok(None) => tracing::info!(%culprit, "forward fix landed; no auto-revert"),
            Err(e) => tracing::error!(%culprit, error = %e, "auto-revert failed"),
        }
    });
}

/// Revert `culprit` as `svc:lander` unless a forward fix has landed.
pub async fn auto_revert(lander: &Arc<Lander>, culprit: TrunkSeq) -> Result<Option<RevertOutcome>> {
    if has_forward_fix(lander, culprit).await? {
        return Ok(None);
    }
    let reason = format!("post-land verification found {culprit} red and no forward fix landed");
    revert_with(
        lander,
        culprit,
        &reason,
        lander_principal(),
        AUTO_REVERT_TOOL,
    )
    .await
    .map(Some)
}
