//! Reviewers' track records in a repository, and the trust they lend
//! the people in a change's facts.

use std::collections::BTreeMap;

use rill_policy::reviewers::ReviewerStats;
use rill_policy::ChangeFacts;

use crate::error::{Error, Result};
use crate::lifecycle::MAX_WRITE_ATTEMPTS;
use crate::service::ReviewService;

const STATS_KEY: &str = "reviewer-stats";

/// Records by agent name.
pub type StatsMap = BTreeMap<String, ReviewerStats>;

fn codec(e: serde_json::Error) -> Error {
    Error::Store(rill_store::Error::Codec(format!("reviewer stats: {e}")))
}

impl ReviewService {
    /// Every reviewer's record in this repository.
    pub async fn reviewer_stats(&self) -> Result<StatsMap> {
        match self.repo.get_misc(STATS_KEY).await? {
            Some(b) => serde_json::from_slice(&b).map_err(codec),
            None => Ok(StatsMap::new()),
        }
    }

    /// Change the records with `f`, retrying when another write came
    /// first.
    pub async fn update_reviewer_stats<T>(
        &self,
        mut f: impl FnMut(&mut StatsMap) -> T,
    ) -> Result<T> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let current = self.repo.get_misc(STATS_KEY).await?;
            let mut map: StatsMap = match &current {
                Some(b) => serde_json::from_slice(b).map_err(codec)?,
                None => StatsMap::new(),
            };
            let out = f(&mut map);
            let value = serde_json::to_vec(&map).map_err(codec)?;
            match self
                .repo
                .set_misc_cas(STATS_KEY, current, value.into())
                .await
            {
                Ok(()) => return Ok(out),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Store(rill_store::Error::Conflict(
            "reviewer stats kept changing".into(),
        )))
    }

    /// Give the reviewers among `facts.people` their trust and standing.
    pub(crate) async fn fill_trust(&self, facts: &mut ChangeFacts) -> Result<()> {
        if !facts.people.values().any(|p| p.reviewer.is_some()) {
            return Ok(());
        }
        let stats = self.reviewer_stats().await?;
        for (name, person) in facts.people.iter_mut() {
            if person.reviewer.is_none() {
                continue;
            }
            let agent = name.strip_prefix("agent:").unwrap_or(name);
            let s = stats.get(agent).cloned().unwrap_or_default();
            person.trust = Some(s.trust());
            person.demoted = s.demoted;
        }
        Ok(())
    }
}
