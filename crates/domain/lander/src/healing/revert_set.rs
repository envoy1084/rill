//! Blast-radius reverts: undo every landing one session, model, sponsor,
//! author or intent made in a range of trunk, newest first. A preview
//! lists what would be reverted and which reverts conflict, without
//! landing anything; applying queues each revert in the emergency lane on
//! its own, so the ones that do not conflict land even when one does.

use rill_core::rebase;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, EventKind, IntentId, Lane, Principal, RepoPathBuf,
    RiskClass, Timestamp, TrunkSeq,
};

use super::fixes::{fixes_of, is_revert};
use crate::config::VerifyMode;
use crate::error::{Error, Result};
use crate::land::Lander;
use crate::queue::{LandRequest, LandTicket};
use crate::revert::{build_revert, REVERT_TOOL};

/// Landings a selector may match at most: past it the range must narrow.
pub const MAX_REVERT_SET: usize = 500;

/// Which landings to revert: all that match every field given, in
/// `(since, until]` (the whole trunk when absent).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pub session: Option<String>,
    pub model: Option<String>,
    /// A sponsor or author, by name (`ann`) or as shown (`agent:bot`).
    pub sponsor: Option<String>,
    pub author: Option<String>,
    pub intent: Option<IntentId>,
    pub since: Option<TrunkSeq>,
    pub until: Option<TrunkSeq>,
}

impl Selector {
    /// Whether it names who or what made the landings; a range alone is
    /// not a blast radius.
    pub fn names_origin(&self) -> bool {
        self.session.is_some()
            || self.model.is_some()
            || self.sponsor.is_some()
            || self.author.is_some()
            || self.intent.is_some()
    }

    fn matches(&self, cv: &ChangeVersion) -> bool {
        let named = |want: &Option<String>, who: Option<&Principal>| match want {
            None => true,
            Some(want) => who.is_some_and(|p| p.name == *want || p.to_string() == *want),
        };
        let equal = |want: &Option<String>, have: &Option<String>| {
            want.as_ref().is_none_or(|want| have.as_ref() == Some(want))
        };
        equal(&self.session, &cv.provenance.session)
            && equal(&self.model, &cv.provenance.model)
            && named(&self.sponsor, cv.provenance.sponsor.as_ref())
            && named(&self.author, Some(&cv.author))
            && self.intent.is_none_or(|intent| cv.intent == Some(intent))
    }

    /// A short form for the audit log.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        for (key, value) in [
            ("session", &self.session),
            ("model", &self.model),
            ("sponsor", &self.sponsor),
            ("author", &self.author),
        ] {
            if let Some(value) = value {
                parts.push(format!("{key}={value}"));
            }
        }
        if let Some(intent) = self.intent {
            parts.push(format!("intent={intent}"));
        }
        if let Some(since) = self.since {
            parts.push(format!("since={since}"));
        }
        if let Some(until) = self.until {
            parts.push(format!("until={until}"));
        }
        parts.join(" ")
    }
}

/// One landing a selector matched, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertEntry {
    pub seq: TrunkSeq,
    pub change: ChangeId,
    pub version: ChangeVersionHash,
    pub summary: String,
    /// Paths its revert conflicts at, after the newer reverts before it.
    pub conflicts: Vec<RepoPathBuf>,
}

impl Lander {
    /// What reverting `selector`'s landings would do, without landing
    /// anything: each landing, newest first, with the paths its revert
    /// would conflict at once the newer ones are reverted.
    pub async fn plan_reverts(&self, selector: &Selector) -> Result<Vec<RevertEntry>> {
        let matched = self.matching(selector).await?;
        let Some((_, head)) = self.repo().trunk_head().await? else {
            return Ok(Vec::new());
        };
        let mut tip = head;
        let mut out = Vec::with_capacity(matched.len());
        for (seq, version, cv) in matched {
            let undo = build_revert(
                self,
                seq,
                "preview",
                Principal::service("preview"),
                REVERT_TOOL,
            )
            .await?;
            let undo_hash = undo.version_hash();
            let rebased = rebase(self.derive(), &undo_hash, &tip, Timestamp::now()).await?;
            let conflicts: Vec<RepoPathBuf> = rebased.conflicted_paths().cloned().collect();
            if conflicts.is_empty() {
                tip = self.derive().store_version(&rebased).await?;
            }
            out.push(RevertEntry {
                seq,
                change: cv.change_id,
                version,
                summary: cv.summary().to_string(),
                conflicts,
            });
        }
        Ok(out)
    }

    /// Queue a revert of every landing `selector` matches, newest first, in
    /// the emergency lane and without pre-land verification, each on its
    /// own. Returns the tickets in that order.
    pub async fn apply_reverts(
        &self,
        selector: &Selector,
        reason: &str,
        actor: &Principal,
    ) -> Result<Vec<(TrunkSeq, LandTicket)>> {
        let matched = self.matching(selector).await?;
        let now = Timestamp::now().millis();
        let mut tickets = Vec::with_capacity(matched.len());
        for (i, (seq, _, _)) in matched.iter().enumerate() {
            let undo = build_revert(self, *seq, reason, actor.clone(), REVERT_TOOL).await?;
            let mut request = LandRequest::new(
                undo.change_id,
                undo.version_hash(),
                actor.clone(),
                RiskClass::Normal,
            )
            .in_lane(Lane::Emergency);
            request.verify_override = Some(VerifyMode::None);
            // Queued a millisecond apart: served newest culprit first.
            request.requested_at = Timestamp::from_millis(now + i as i64);
            tickets.push((*seq, self.enqueue(request).await?));
        }
        let mut tx = rill_store::Transaction::new();
        tx.mutations.extend(self.repo().event_by(
            actor,
            EventKind::RevertsRequested {
                selector: selector.describe(),
                count: tickets.len() as u32,
            },
        )?);
        self.repo().meta().transact(tx).await?;
        tracing::warn!(selector = %selector.describe(), count = tickets.len(), by = %actor, "mass revert requested");
        Ok(tickets)
    }

    /// The landings `selector` matches that are not reverted yet and are
    /// not reverts themselves, newest first.
    async fn matching(
        &self,
        selector: &Selector,
    ) -> Result<Vec<(TrunkSeq, ChangeVersionHash, ChangeVersion)>> {
        if !selector.names_origin() {
            return Err(Error::Config(
                "a mass revert names a session, model, sponsor, author or intent".into(),
            ));
        }
        let repo = self.repo();
        let Some((head, _)) = repo.trunk_head().await? else {
            return Ok(Vec::new());
        };
        let until = selector.until.unwrap_or(head).min(head);
        let since = selector.since.unwrap_or(TrunkSeq::ZERO);
        let mut matched = Vec::new();
        let mut reverted = std::collections::BTreeSet::new();
        for (seq, version) in repo.trunk_range(TrunkSeq::ZERO, head).await? {
            let cv = self.derive().change(&version).await?;
            if is_revert(&cv) {
                reverted.extend(fixes_of(&cv));
                continue;
            }
            if seq > since && seq <= until && selector.matches(&cv) {
                matched.push((seq, version, cv));
            }
        }
        matched.retain(|(seq, _, _)| !reverted.contains(seq));
        if matched.len() > MAX_REVERT_SET {
            return Err(Error::Config(format!(
                "{} landings match; narrow the range to at most {MAX_REVERT_SET}",
                matched.len()
            )));
        }
        matched.reverse();
        Ok(matched)
    }
}
