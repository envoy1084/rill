//! `.rill/verify.toml`: the steps that verify a tree, versioned with it.
//!
//! ```toml
//! [[step]]
//! name = "unit"
//! command = ["cargo", "test", "--workspace", "--locked"]
//! inputs = ["**/*.rs", "**/Cargo.toml", "Cargo.lock"]
//! working_dir = "."
//! env = { RUST_BACKTRACE = "1" }
//! outputs = ["report.xml"]           # relative to working_dir
//! timeout_secs = 900
//! platform = { OSFamily = "linux" }  # added to the server's default
//! shards = 4
//! risk = ["normal", "sensitive", "critical"]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use rill_core::{read_content, TreeOps};
use rill_types::{EntryKind, RepoPathBuf, RiskClass, TreeHash};
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::remote::InputFilter;

/// Where the plan lives inside a tree.
pub const PLAN_PATH: &str = ".rill/verify.toml";

/// Timeout of a step that declares none.
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(600);

/// Most shards a step may ask for.
pub const MAX_SHARDS: u32 = 256;

/// One named check.
#[derive(Debug, Clone)]
pub struct Step {
    pub name: String,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub working_dir: RepoPathBuf,
    /// Files the step reads; the rest of the tree is not in its input
    /// root, so changes elsewhere neither run it nor miss its cache.
    pub inputs: InputFilter,
    /// Output files, relative to the working directory.
    pub outputs: Vec<String>,
    pub timeout: Duration,
    /// Platform properties added to the server's defaults.
    pub platform: BTreeMap<String, String>,
    /// Parallel parts, each told its index through the test sharding
    /// environment (`TEST_SHARD_INDEX`, `TEST_TOTAL_SHARDS`).
    pub shards: u32,
    /// Risk classes the step gates; `None` is every class.
    pub risk: Option<BTreeSet<RiskClass>>,
}

/// The steps that verify a tree.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    /// Parse a plan from TOML.
    pub fn parse(text: &str) -> Result<Self> {
        let file: PlanFile = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        let mut names = BTreeSet::new();
        let mut steps = Vec::with_capacity(file.step.len());
        for (index, step) in file.step.into_iter().enumerate() {
            let name = match step.name {
                Some(name) if !name.is_empty() => name,
                _ => format!("step-{index}"),
            };
            if step.command.is_empty() {
                return Err(invalid(format!("step {name:?} has an empty command")));
            }
            if !names.insert(name.clone()) {
                return Err(invalid(format!("two steps are named {name:?}")));
            }
            let shards = step.shards.unwrap_or(1);
            if !(1..=MAX_SHARDS).contains(&shards) {
                return Err(invalid(format!(
                    "step {name:?} asks for {shards} shards (1 to {MAX_SHARDS})"
                )));
            }
            let working_dir = match step.working_dir.as_deref() {
                None | Some(".") | Some("") => RepoPathBuf::root(),
                Some(dir) => RepoPathBuf::new(dir)
                    .map_err(|e| invalid(format!("step {name:?} working_dir: {e}")))?,
            };
            for output in &step.outputs {
                RepoPathBuf::new(output)
                    .map_err(|e| invalid(format!("step {name:?} output {output:?}: {e}")))?;
            }
            let risk = match step.risk {
                None => None,
                Some(classes) => Some(
                    classes
                        .iter()
                        .map(|c| c.parse::<RiskClass>())
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|e| invalid(format!("step {name:?}: {e}")))?,
                ),
            };
            steps.push(Step {
                inputs: InputFilter::new(&step.inputs)?,
                command: step.command,
                env: step.env,
                working_dir,
                outputs: step.outputs,
                timeout: step
                    .timeout_secs
                    .map(Duration::from_secs)
                    .unwrap_or(DEFAULT_STEP_TIMEOUT),
                platform: step.platform,
                shards,
                risk,
                name,
            });
        }
        Ok(Plan { steps })
    }
}

fn invalid(reason: String) -> Error {
    Error::Config {
        path: PLAN_PATH.to_string(),
        reason,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanFile {
    #[serde(default)]
    step: Vec<StepFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepFile {
    name: Option<String>,
    command: Vec<String>,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    outputs: Vec<String>,
    working_dir: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    timeout_secs: Option<u64>,
    #[serde(default)]
    platform: BTreeMap<String, String>,
    shards: Option<u32>,
    risk: Option<Vec<String>>,
}

/// The plan at [`PLAN_PATH`] in `root`, or `None` when the tree has none.
pub async fn load_plan(trees: &TreeOps, root: TreeHash) -> Result<Option<Plan>> {
    let path = RepoPathBuf::new(PLAN_PATH)?;
    let Some(EntryKind::File { content, .. }) = trees.lookup(root, &path).await? else {
        return Ok(None);
    };
    let bytes = read_content(trees.objects().as_ref(), &content).await?;
    let text = std::str::from_utf8(&bytes).map_err(|e| invalid(format!("not UTF-8: {e}")))?;
    Ok(Some(Plan::parse(text)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_field() {
        let plan = Plan::parse(
            r#"
[[step]]
name = "unit"
command = ["cargo", "test"]
inputs = ["src/**"]
outputs = ["out/report.xml"]
working_dir = "crates/x"
env = { RUST_LOG = "debug" }
timeout_secs = 30
platform = { OSFamily = "linux" }
shards = 4
risk = ["normal", "critical"]

[[step]]
command = ["true"]
"#,
        )
        .unwrap();
        let unit = &plan.steps[0];
        assert_eq!(unit.working_dir.as_str(), "crates/x");
        assert_eq!(unit.timeout, Duration::from_secs(30));
        assert_eq!(unit.shards, 4);
        assert_eq!(
            unit.risk,
            Some(BTreeSet::from([RiskClass::Normal, RiskClass::Critical]))
        );
        let bare = &plan.steps[1];
        assert_eq!(bare.name, "step-1");
        assert!(bare.working_dir.is_root());
        assert_eq!((bare.shards, bare.timeout), (1, DEFAULT_STEP_TIMEOUT));
        assert!(bare.risk.is_none());
    }

    #[test]
    fn rejects_plans_that_cannot_run_as_written() {
        for text in [
            "[[step]]\ncommand = []\n",
            "[[step]]\nname = \"a\"\ncommand = [\"x\"]\n[[step]]\nname = \"a\"\ncommand = [\"y\"]\n",
            "[[step]]\ncommand = [\"x\"]\nshards = 0\n",
            "[[step]]\ncommand = [\"x\"]\nrisk = [\"spicy\"]\n",
            "[[step]]\ncommand = [\"x\"]\ninput = [\"typo\"]\n",
            "[[step]]\ncommand = [\"x\"]\noutputs = [\"../up\"]\n",
        ] {
            assert!(Plan::parse(text).is_err(), "{text}");
        }
        assert!(Plan::parse("").unwrap().steps.is_empty());
    }
}
