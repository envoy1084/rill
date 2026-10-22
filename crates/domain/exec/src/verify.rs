//! Repository-defined verification plans.
//!
//! A repository declares how to verify itself in `.rill/verify.toml` at
//! the tree being verified, so the plan is versioned with the code it
//! checks:
//!
//! ```toml
//! [[step]]
//! name = "unit"
//! command = ["cargo", "test"]
//! timeout_secs = 900
//! working_dir = "crates/core"   # optional, defaults to the root
//! env = { RUST_BACKTRACE = "1" } # optional
//! ```
//!
//! A tree with no plan is trivially green: verification is opt-in per
//! repository, and an absent file must not block landing.

use std::collections::BTreeMap;
use std::time::Duration;

use rill_core::{read_content, TreeOps};
use rill_types::{EntryKind, RepoPathBuf, TreeHash};
use serde::Deserialize;

use crate::action::{host_platform, Action, ActionResult};
use crate::error::{Error, Result};
use crate::runner::{CacheStatus, Runner};

/// Where the plan lives inside a tree.
pub const PLAN_PATH: &str = ".rill/verify.toml";

/// Timeout applied to a step that does not declare one.
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(600);

/// One named action of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub name: String,
    pub action: Action,
}

/// The actions that verify a tree, in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerificationPlan {
    pub steps: Vec<PlanStep>,
}

impl VerificationPlan {
    /// Parse a plan from TOML, binding every step to `root`.
    pub fn parse(text: &str, root: TreeHash) -> Result<Self> {
        let file: PlanFile = toml::from_str(text).map_err(|e| Error::Config {
            path: PLAN_PATH.to_string(),
            reason: e.to_string(),
        })?;
        let platform = host_platform();
        let mut steps = Vec::with_capacity(file.step.len());
        for (index, step) in file.step.into_iter().enumerate() {
            if step.command.is_empty() {
                return Err(Error::Config {
                    path: PLAN_PATH.to_string(),
                    reason: format!("step {} ({:?}) has an empty command", index, step.name),
                });
            }
            let name = if step.name.is_empty() {
                format!("step-{index}")
            } else {
                step.name
            };
            let working_dir = match step.working_dir {
                Some(dir) => RepoPathBuf::new(dir)?,
                None => RepoPathBuf::root(),
            };
            let mut output_paths = Vec::with_capacity(step.outputs.len());
            for output in step.outputs {
                output_paths.push(RepoPathBuf::new(output)?);
            }
            steps.push(PlanStep {
                name,
                action: Action {
                    command: step.command,
                    env: step.env,
                    input_root: root,
                    working_dir,
                    timeout: step
                        .timeout_secs
                        .map(Duration::from_secs)
                        .unwrap_or(DEFAULT_STEP_TIMEOUT),
                    platform: platform.clone(),
                    output_paths,
                },
            });
        }
        Ok(VerificationPlan { steps })
    }
}

#[derive(Debug, Deserialize)]
struct PlanFile {
    #[serde(default)]
    step: Vec<StepFile>,
}

#[derive(Debug, Deserialize)]
struct StepFile {
    #[serde(default)]
    name: String,
    command: Vec<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    outputs: Vec<String>,
}

/// Load the plan stored at [`PLAN_PATH`] in `root`, or `None` when the
/// tree has no plan.
pub async fn load_plan(trees: &TreeOps, root: TreeHash) -> Result<Option<VerificationPlan>> {
    let path = RepoPathBuf::new(PLAN_PATH)?;
    let Some(EntryKind::File { content, .. }) = trees.lookup(root, &path).await? else {
        return Ok(None);
    };
    let bytes = read_content(trees.objects().as_ref(), &content).await?;
    let text = std::str::from_utf8(&bytes).map_err(|e| Error::Config {
        path: PLAN_PATH.to_string(),
        reason: format!("not UTF-8: {e}"),
    })?;
    Ok(Some(VerificationPlan::parse(text, root)?))
}

/// Result of one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub name: String,
    pub result: ActionResult,
    /// True when the result came from the action cache.
    pub cached: bool,
}

/// Result of verifying a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationOutcome {
    /// True when every step succeeded (or there were none).
    pub green: bool,
    pub steps: Vec<StepResult>,
}

impl VerificationOutcome {
    /// The first failing step, if any.
    pub fn first_failure(&self) -> Option<&StepResult> {
        self.steps.iter().find(|s| !s.result.succeeded())
    }
}

/// Run `plan` with `runner`, stopping at the first failing step.
pub async fn verify_plan(runner: &Runner, plan: &VerificationPlan) -> Result<VerificationOutcome> {
    let mut steps = Vec::with_capacity(plan.steps.len());
    let mut green = true;
    for step in &plan.steps {
        let (result, status) = runner.execute(&step.action).await?;
        let ok = result.succeeded();
        tracing::info!(step = %step.name, exit_code = result.exit_code, cached = status == CacheStatus::Hit, "verification step");
        steps.push(StepResult {
            name: step.name.clone(),
            result,
            cached: status == CacheStatus::Hit,
        });
        if !ok {
            green = false;
            break;
        }
    }
    Ok(VerificationOutcome { green, steps })
}

/// Verify `root` according to the plan it contains. A tree without a
/// plan is green.
pub async fn verify_tree(runner: &Runner, root: TreeHash) -> Result<VerificationOutcome> {
    match load_plan(runner.trees(), root).await? {
        Some(plan) => verify_plan(runner, &plan).await,
        None => Ok(VerificationOutcome {
            green: true,
            steps: Vec::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use rill_types::ObjectHash;

    use super::*;

    #[test]
    fn parses_steps() {
        let text = r#"
[[step]]
name = "unit"
command = ["cargo", "test", "--workspace"]
timeout_secs = 30
working_dir = "crates/x"
env = { RUST_LOG = "debug" }
outputs = ["target/report.xml"]

[[step]]
command = ["sh", "-c", "true"]
"#;
        let plan = VerificationPlan::parse(text, ObjectHash::ZERO).unwrap();
        assert_eq!(plan.steps.len(), 2);
        let unit = &plan.steps[0];
        assert_eq!(unit.name, "unit");
        assert_eq!(unit.action.command, vec!["cargo", "test", "--workspace"]);
        assert_eq!(unit.action.timeout, Duration::from_secs(30));
        assert_eq!(unit.action.working_dir.as_str(), "crates/x");
        assert_eq!(unit.action.env.get("RUST_LOG").unwrap(), "debug");
        assert_eq!(unit.action.output_paths[0].as_str(), "target/report.xml");
        assert_eq!(plan.steps[1].name, "step-1");
        assert_eq!(plan.steps[1].action.timeout, DEFAULT_STEP_TIMEOUT);
        assert!(plan.steps[1].action.working_dir.is_root());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn verifies_a_tree_end_to_end() {
        use std::sync::Arc;

        use rill_core::tree::{Edit, TreeBuilder};
        use rill_store::Stores;
        use rill_types::{ContentRef, FileKind};

        use crate::sandbox::LocalSandbox;

        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), 16);
        let runner = Runner::new(repo, trees.clone(), Arc::new(LocalSandbox::default()));

        let plan =
            "[[step]]\nname = \"check\"\ncommand = [\"sh\", \"-c\", \"test -f hello.txt\"]\n";
        let mut b = TreeBuilder::new();
        b.edit(
            &RepoPathBuf::new(PLAN_PATH).unwrap(),
            Edit::Set(
                ContentRef::inline(plan.as_bytes().to_vec()),
                FileKind::Regular,
            ),
        );
        let red = b.build(&trees, None).await.unwrap();
        let mut b = TreeBuilder::new();
        b.edit(
            &RepoPathBuf::new("hello.txt").unwrap(),
            Edit::Set(ContentRef::inline(b"hi".to_vec()), FileKind::Regular),
        );
        let green = b.build(&trees, Some(red)).await.unwrap();

        let outcome = verify_tree(&runner, red).await.unwrap();
        assert!(!outcome.green);
        assert_eq!(outcome.first_failure().unwrap().name, "check");
        let outcome = verify_tree(&runner, green).await.unwrap();
        assert!(outcome.green);
        assert!(!outcome.steps[0].cached);
        assert!(verify_tree(&runner, green).await.unwrap().steps[0].cached);

        let empty = trees.empty_tree().await.unwrap();
        let outcome = verify_tree(&runner, empty).await.unwrap();
        assert!(outcome.green && outcome.steps.is_empty());
    }

    #[test]
    fn rejects_bad_plans() {
        assert!(VerificationPlan::parse("[[step]]\ncommand = []\n", ObjectHash::ZERO).is_err());
        assert!(VerificationPlan::parse("[[step]]\nname = 1\n", ObjectHash::ZERO).is_err());
        assert!(VerificationPlan::parse("", ObjectHash::ZERO)
            .unwrap()
            .steps
            .is_empty());
    }
}
