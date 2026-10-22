//! `[graph]`: the action that lists a repository's build targets and
//! their hashes.
//!
//! ```toml
//! [graph]
//! tool = "bazel"                      # or "buck2"; presets command and output
//! command = ["bazel-diff", "..."]     # overrides the preset
//! output = "target-hashes.json"
//! inputs = ["**"]                     # default: every file
//! platform = { network = "fetch" }
//! timeout_secs = 1800
//! ```

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::remote::InputFilter;
use crate::verify::plan::{Step, PLAN_PATH};

/// The step name graph queries report under.
pub const GRAPH_STEP: &str = "build-graph";
/// Where the presets write their hashes.
pub const DEFAULT_OUTPUT: &str = "target-hashes.json";
/// Timeout of a graph query that declares none.
pub const DEFAULT_GRAPH_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How to compute target hashes, as a step whose one output is them.
#[derive(Debug, Clone)]
pub struct GraphSpec {
    pub step: Step,
    /// The output holding the hashes, relative to the working directory.
    pub output: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GraphFile {
    tool: Option<String>,
    command: Option<Vec<String>>,
    output: Option<String>,
    #[serde(default)]
    inputs: Vec<String>,
    working_dir: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    timeout_secs: Option<u64>,
    #[serde(default)]
    platform: BTreeMap<String, String>,
}

impl GraphFile {
    pub(crate) fn into_spec(self) -> Result<GraphSpec> {
        let output = self.output.unwrap_or_else(|| DEFAULT_OUTPUT.to_string());
        let preset = match self.tool.as_deref() {
            None => None,
            Some("bazel") => Some(vec![
                "bazel-diff".to_string(),
                "generate-hashes".into(),
                "-w".into(),
                ".".into(),
                "-b".into(),
                "bazel".into(),
                output.clone(),
            ]),
            Some("buck2") => Some(vec![
                "sh".to_string(),
                "-c".into(),
                format!("buck2 targets //... --show-target-hash --json > '{output}'"),
            ]),
            Some(other) => return Err(invalid(format!("unknown graph tool {other:?}"))),
        };
        let command = self
            .command
            .or(preset)
            .filter(|c| !c.is_empty())
            .ok_or_else(|| invalid("[graph] needs a tool or a command".into()))?;
        let working_dir = match self.working_dir.as_deref() {
            None | Some(".") | Some("") => rill_types::RepoPathBuf::root(),
            Some(dir) => rill_types::RepoPathBuf::new(dir)
                .map_err(|e| invalid(format!("[graph] working_dir: {e}")))?,
        };
        rill_types::RepoPathBuf::new(&output)
            .map_err(|e| invalid(format!("[graph] output {output:?}: {e}")))?;
        Ok(GraphSpec {
            step: Step {
                name: GRAPH_STEP.into(),
                command,
                env: self.env,
                working_dir,
                inputs: InputFilter::new(&self.inputs)?,
                outputs: vec![output.clone()],
                timeout: self
                    .timeout_secs
                    .map(Duration::from_secs)
                    .unwrap_or(DEFAULT_GRAPH_TIMEOUT),
                platform: self.platform,
                shards: 1,
                risk: None,
                affected_targets: false,
                secrets: Vec::new(),
            },
            output,
        })
    }
}

fn invalid(reason: String) -> Error {
    Error::Config {
        path: PLAN_PATH.to_string(),
        reason,
    }
}
