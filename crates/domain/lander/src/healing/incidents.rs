//! Opening, moving and closing incidents, and keeping trunk's health in
//! step with them.

use rill_types::{
    ChangeVersion, Incident, IncidentState, Resolution, Timestamp, TrunkHealth, TrunkSeq,
};

use super::fixes::{fixes_of, is_revert};
use super::state::{judge, GuardRules};
use crate::error::Result;
use crate::land::Lander;
use crate::revert::lander_principal;
use crate::traits::Failure;

/// Incidents read when judging health; far more than are ever open.
const RECENT_INCIDENTS: usize = 256;

impl Lander {
    /// Open an incident for `culprit`, which failed with `failures`
    /// (`details` from its verification), tell its owner, offer it to a fix
    /// agent and judge trunk again. An incident already open for it is
    /// returned as it is.
    pub async fn open_incident(
        &self,
        culprit: TrunkSeq,
        failures: &[Failure],
        details: &str,
    ) -> Result<Incident> {
        let repo = self.repo();
        if let Some(existing) = repo.incident(culprit).await? {
            if existing.is_open() {
                return Ok(existing);
            }
        }
        let version = repo
            .trunk_at(culprit)
            .await?
            .ok_or(crate::error::Error::NotLanded(culprit))?;
        let cv = self.derive().change(&version).await?;
        let mut steps: Vec<String> = failures.iter().map(|f| f.step.clone()).collect();
        steps.sort();
        steps.dedup();
        let now = Timestamp::now();
        let mut incident = Incident {
            culprit,
            version,
            change: cv.change_id,
            failures: failures.iter().map(|f| f.target.clone()).collect(),
            steps,
            details: details.to_string(),
            author: cv.author.clone(),
            session: cv.provenance.session.clone(),
            model: cv.provenance.model.clone(),
            sponsor: cv.provenance.sponsor.clone(),
            state: IncidentState::Open,
            opened_at: now,
            updated_at: now,
            fix: None,
            resolution: None,
            revert_blocked: None,
            assigned: None,
        };
        if let Some(fixer) = &self.fixer {
            match fixer.dispatch(&incident).await {
                Ok(assigned) => incident.assigned = assigned,
                Err(e) => tracing::warn!(%culprit, error = %e, "dispatching a fix agent failed"),
            }
        }
        repo.put_incident(&incident, &lander_principal()).await?;
        tracing::warn!(%culprit, steps = ?incident.steps, "trunk incident opened");
        if let Err(e) = self.hooks.notify_incident(cv.change_id, &incident).await {
            tracing::warn!(%culprit, error = %e, "notifying the culprit's owner failed");
        }
        self.refresh_health().await?;
        Ok(incident)
    }

    /// Note what `landed` fixes: open incidents whose culprit it reverts
    /// or names as fixed move to `Fixing`.
    pub async fn note_fixes(&self, landed: &[(TrunkSeq, ChangeVersion)]) -> Result<()> {
        let mut moved = false;
        for (seq, cv) in landed {
            for culprit in fixes_of(cv) {
                let Some(mut incident) = self.repo().incident(culprit).await? else {
                    continue;
                };
                if incident.state != IncidentState::Open {
                    continue;
                }
                incident.state = IncidentState::Fixing;
                incident.fix = Some(*seq);
                incident.resolution = Some(match is_revert(cv) {
                    true => Resolution::Reverted(*seq),
                    false => Resolution::Fixed(*seq),
                });
                incident.updated_at = Timestamp::now();
                self.repo()
                    .put_incident(&incident, &lander_principal())
                    .await?;
                moved = true;
            }
        }
        if moved {
            self.refresh_health().await?;
        }
        Ok(())
    }

    /// Trunk passed at `at`: close every open incident.
    pub async fn close_incidents(&self, at: TrunkSeq) -> Result<()> {
        let open = self.open_incidents().await?;
        if open.is_empty() {
            return Ok(());
        }
        for mut incident in open {
            incident.state = IncidentState::Closed;
            incident.resolution = incident.resolution.or(Some(Resolution::Recovered(at)));
            incident.updated_at = Timestamp::now();
            self.repo()
                .put_incident(&incident, &lander_principal())
                .await?;
            tracing::info!(culprit = %incident.culprit, "trunk incident closed");
        }
        self.refresh_health().await?;
        Ok(())
    }

    /// The automatic revert of `culprit` could not land: record why and
    /// tell the culprit's owner, once.
    pub async fn revert_blocked(&self, culprit: TrunkSeq, reason: &str) -> Result<()> {
        let Some(mut incident) = self.repo().incident(culprit).await? else {
            return Ok(());
        };
        if !incident.is_open() || incident.revert_blocked.is_some() {
            return Ok(());
        }
        incident.revert_blocked = Some(reason.to_string());
        incident.updated_at = Timestamp::now();
        self.repo()
            .put_incident(&incident, &lander_principal())
            .await?;
        if let Err(e) = self.hooks.notify_incident(incident.change, &incident).await {
            tracing::warn!(%culprit, error = %e, "notifying a blocked revert failed");
        }
        Ok(())
    }

    /// Freeze trunk (only emergency landings land) or thaw it, by `actor`.
    pub async fn set_frozen(
        &self,
        frozen: bool,
        reason: Option<String>,
        actor: &rill_types::Principal,
    ) -> Result<TrunkHealth> {
        let mut health = self.repo().trunk_health().await?;
        health.frozen = frozen;
        health.frozen_reason = reason.filter(|_| frozen);
        self.repo().set_trunk_health(&health, actor).await?;
        tracing::warn!(frozen, by = %actor, "trunk freeze changed");
        Ok(health)
    }

    /// Incidents not closed, newest culprit first.
    pub async fn open_incidents(&self) -> Result<Vec<Incident>> {
        Ok(self
            .repo()
            .incidents(RECENT_INCIDENTS)
            .await?
            .into_iter()
            .filter(Incident::is_open)
            .collect())
    }

    /// Judge trunk's health from its open incidents now, and store it if
    /// it changed.
    pub async fn refresh_health(&self) -> Result<TrunkHealth> {
        let previous = self.repo().trunk_health().await?;
        let open = self.open_incidents().await?;
        let rules = GuardRules {
            after: self.policy().guard_after,
            incidents: self.policy().guard_incidents,
        };
        let next = judge(&previous, &open, Timestamp::now(), rules);
        if next != previous {
            self.repo()
                .set_trunk_health(&next, &lander_principal())
                .await?;
            if next.state != previous.state {
                tracing::info!(from = ?previous.state, to = ?next.state, "trunk health changed");
            }
        }
        Ok(next)
    }
}
