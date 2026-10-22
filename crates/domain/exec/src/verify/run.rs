//! Verifying a tree: select the steps a change needs, compile them to
//! actions, and run them through the scheduler.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use bytes::Bytes;
use futures::stream::{FuturesUnordered, StreamExt};
use rill_core::diff::diff_trees;
use rill_core::TreeOps;
use rill_jobs::Priority;
use rill_types::{ObjectHash, RiskClass, TreeHash};

use super::compile::{compile, ExtraFile, StepDefaults};
use super::graph::{affected_targets, GraphSpec};
use super::outcome::{StepResult, StepStatus, VerificationOutcome};
use super::plan::{load_plan, Step};
use super::progress::Progress;
use super::select::{select, Selection};
use crate::error::Result;
use crate::history::TestHistory;
use crate::remote::{ExecSpace, Scheduler};
use crate::runs::{RunStore, StepRecord};

/// The file, in a step's working directory, listing affected targets.
pub const AFFECTED_FILE: &str = ".rill-affected-targets";
/// The variable naming [`AFFECTED_FILE`] to the command.
pub const AFFECTED_ENV: &str = "RILL_AFFECTED_TARGETS";

/// A tree to verify.
#[derive(Debug, Clone)]
pub struct Target {
    pub root: TreeHash,
    /// The version whose root it is, for the run record.
    pub version: ObjectHash,
    /// The tree the change was made against. Steps whose inputs did not
    /// change since it are not run; `None` runs every step.
    pub base: Option<TreeHash>,
    /// The change's risk class; `None` runs every step whatever it gates.
    pub risk: Option<RiskClass>,
    pub priority: Priority,
    /// The tree is trunk, verified after landing; secrets limited to
    /// trunk are given only then.
    pub trunk: bool,
}

/// Runs a repository's plans in its org's execution space.
pub struct PlanRunner {
    pub(crate) scheduler: Arc<Scheduler>,
    pub(crate) space: ExecSpace,
    pub(crate) trees: TreeOps,
    pub(crate) defaults: StepDefaults,
    pub(crate) repo: String,
    pub(crate) requester: String,
    pub(crate) runs: Option<RunStore>,
    /// Whose quarantined targets do not fail a step.
    pub(crate) quarantine: Option<TestHistory>,
}

impl PlanRunner {
    /// A runner for `repo`'s trees, read through `trees`, submitting in
    /// `org`'s space with `platform` as every step's base properties.
    pub fn new(
        scheduler: Arc<Scheduler>,
        org: &str,
        repo: &str,
        trees: TreeOps,
        platform: BTreeMap<String, String>,
    ) -> Self {
        PlanRunner {
            space: scheduler.space(org),
            scheduler,
            trees,
            defaults: StepDefaults {
                platform,
                env: BTreeMap::new(),
            },
            repo: repo.to_string(),
            requester: format!("verify:{repo}"),
            runs: None,
            quarantine: None,
        }
    }

    /// Do not fail a step on targets `history` has quarantined.
    pub fn with_quarantine(mut self, history: TestHistory) -> Self {
        self.quarantine = Some(history);
        self
    }

    /// Give every step `env`, under its own environment. Workers start
    /// commands with only the action's environment, so this is where
    /// `PATH` comes from.
    pub fn with_env(mut self, env: BTreeMap<String, String>) -> Self {
        self.defaults.env = env;
        self
    }

    /// Record each verification as a run in `runs`.
    pub fn with_runs(mut self, runs: RunStore) -> Self {
        self.runs = Some(runs);
        self
    }

    /// Verify `target` by its own plan. A tree without a plan is green;
    /// the first failing step ends verification.
    pub async fn verify(&self, target: &Target) -> Result<VerificationOutcome> {
        let Some(plan) = load_plan(&self.trees, target.root).await? else {
            return Ok(VerificationOutcome {
                green: true,
                ..Default::default()
            });
        };
        let run = match &self.runs {
            Some(runs) => Some(runs.start(target.version, target.trunk).await?),
            None => None,
        };
        let progress = Progress::new(run);
        let changed = match target.base {
            Some(base) => Some(
                diff_trees(&self.trees, Some(base), Some(target.root))
                    .await?
                    .into_iter()
                    .map(|entry| entry.path)
                    .collect::<Vec<_>>(),
            ),
            None => None,
        };
        let mut steps = Vec::new();
        let mut selected: Vec<&Step> = Vec::new();
        for step in &plan.steps {
            match select(step, changed.as_deref(), target.risk) {
                Selection::Run => selected.push(step),
                Selection::Unaffected => {
                    steps.push(StepResult::skipped(&step.name, StepStatus::Unaffected))
                }
                Selection::NotGating => {
                    steps.push(StepResult::skipped(&step.name, StepStatus::NotGating))
                }
            }
        }
        for skipped in &steps {
            progress.ended(skipped).await;
        }

        for step in &selected {
            if step.secrets.is_empty() {
                continue;
            }
            let refusal = self
                .scheduler
                .secret_refusal(&self.space.org, &step.secrets, &self.repo, target.trunk)
                .await?;
            if let Some(reason) = refusal {
                let refused = StepResult::skipped(&step.name, StepStatus::Error(reason));
                progress.ended(&refused).await;
                steps.push(refused);
                return self.finish(false, steps, &progress).await;
            }
        }

        let affected = match plan.graph.as_ref() {
            Some(graph) if selected.iter().any(|s| s.affected_targets) => {
                match self.affected(graph, target).await? {
                    Ok(list) => list,
                    Err(failed) => {
                        progress.ended(&failed).await;
                        steps.push(failed);
                        return self.finish(false, steps, &progress).await;
                    }
                }
            }
            _ => Vec::new(),
        };

        let mut compiling = FuturesUnordered::new();
        for step in selected {
            let extra = if step.affected_targets {
                if affected.is_empty() {
                    let skipped = StepResult::skipped(&step.name, StepStatus::Unaffected);
                    progress.ended(&skipped).await;
                    steps.push(skipped);
                    continue;
                }
                Some(ExtraFile {
                    name: AFFECTED_FILE.into(),
                    env: AFFECTED_ENV.into(),
                    bytes: Bytes::from(affected.join("\n") + "\n"),
                })
            } else {
                None
            };
            compiling.push(compile(
                &self.space,
                &self.trees,
                target.root,
                step,
                &self.defaults,
                extra,
            ));
        }
        let quarantined: HashSet<String> = match &self.quarantine {
            Some(history) => history
                .quarantined()
                .await?
                .into_iter()
                .map(|q| q.target)
                .collect(),
            None => HashSet::new(),
        };
        let mut running = FuturesUnordered::new();
        while let Some(actions) = compiling.next().await {
            for action in actions? {
                let index = progress.index();
                let (progress, quarantined) = (&progress, &quarantined);
                running.push(async move {
                    let (mut result, _) = self
                        .submit(action, target.priority, Some((progress, index)))
                        .await?;
                    result.excuse(quarantined);
                    progress.write(index, &StepRecord::finished(&result)).await;
                    Ok::<_, crate::error::Error>(result)
                });
            }
        }
        while let Some(result) = running.next().await {
            let result = result?;
            let ok = result.status.is_ok();
            steps.push(result);
            if !ok {
                return self.finish(false, steps, &progress).await;
            }
        }
        self.finish(true, steps, &progress).await
    }

    /// The build targets `target` affects against its base: every target
    /// when there is no base or the base's query fails.
    async fn affected(
        &self,
        graph: &GraphSpec,
        target: &Target,
    ) -> Result<std::result::Result<Vec<String>, StepResult>> {
        let after = self.target_hashes(graph, target.root, target.priority);
        let before = async {
            match target.base {
                Some(base) => self
                    .target_hashes(graph, base, target.priority)
                    .await
                    .map(Some),
                None => Ok(None),
            }
        };
        let (after, before) = futures::try_join!(after, before)?;
        let after = match after {
            Ok(hashes) => hashes,
            Err(failed) => return Ok(Err(failed)),
        };
        let before = before.and_then(|query| query.ok());
        Ok(Ok(affected_targets(before.as_ref(), &after)))
    }

    async fn finish(
        &self,
        green: bool,
        steps: Vec<StepResult>,
        progress: &Progress,
    ) -> Result<VerificationOutcome> {
        let mut outcome = VerificationOutcome {
            green,
            steps,
            excerpt: None,
            run: progress.run_id(),
        };
        if let Some(failed) = outcome.first_failure() {
            outcome.excerpt = self.excerpt(failed).await?;
        }
        progress.finish(green, outcome.summary()).await;
        Ok(outcome)
    }
}
