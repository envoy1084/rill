//! The intent a change is linked to, as policy facts: approved, and
//! whether the change stays inside the approved scope and budgets.

use rill_policy::intents::{
    check_scope, day_key, day_of, missing_approvals, DayCounts, IntentFacts,
};
use rill_policy::owners::OwnersIndex;
use rill_policy::ChangeFacts;
use rill_types::{IntentId, RepoPath, Timestamp};

use crate::error::Result;
use crate::service::ReviewService;

impl ReviewService {
    /// Facts about `intent` for a change with `facts`; `None` when the
    /// intent does not exist.
    pub(crate) async fn intent_facts(
        &self,
        intent: &IntentId,
        facts: &ChangeFacts,
        owners: &OwnersIndex,
    ) -> Result<Option<IntentFacts>> {
        let Some(intent) = self.repo.get_intent(intent).await? else {
            return Ok(None);
        };
        let approved = missing_approvals(&intent, owners).is_some_and(|m| m.is_empty());
        let now = Timestamp::now();
        let today = self.intent_day(&intent.id, &day_of(now)).await?;
        let paths: Vec<&RepoPath> = facts.paths.iter().map(|p| p.path.as_path()).collect();
        Ok(Some(check_scope(
            &intent,
            approved,
            &paths,
            facts.total_files,
            today,
            now,
        )?))
    }

    /// How much of `intent`'s budget for `day` is used.
    pub async fn intent_day(&self, intent: &IntentId, day: &str) -> Result<DayCounts> {
        Ok(match self.repo.get_misc(&day_key(intent, day)).await? {
            Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            None => DayCounts::default(),
        })
    }
}
