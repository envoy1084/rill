//! Rebuilding the files a `regenerate` merge driver owns: one step run at
//! the root of the merged tree, whose outputs are those files. Like any
//! step it is sandboxed, has no network and is cached by its inputs.

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use rill_jobs::Priority;
use rill_types::{RepoPathBuf, TreeHash};

use super::compile::compile;
use super::plan::Step;
use super::run::PlanRunner;
use crate::error::{Error, Result};
use crate::remote::InputFilter;

/// A regenerate command and the files it rebuilds.
#[derive(Debug, Clone)]
pub struct Regeneration {
    /// The driver's name; the step is `merge:<name>`.
    pub name: String,
    pub command: Vec<String>,
    /// Globs of the files it reads; empty is the whole tree.
    pub inputs: Vec<String>,
    pub outputs: Vec<RepoPathBuf>,
    pub env: BTreeMap<String, String>,
    pub platform: BTreeMap<String, String>,
    pub timeout: Duration,
}

impl PlanRunner {
    /// Run `job` on `root` and return its outputs, or why it produced
    /// none.
    pub async fn regenerate(
        &self,
        root: TreeHash,
        job: &Regeneration,
    ) -> Result<std::result::Result<BTreeMap<RepoPathBuf, Bytes>, String>> {
        let step = Step {
            name: format!("merge:{}", job.name),
            command: job.command.clone(),
            env: job.env.clone(),
            working_dir: RepoPathBuf::root(),
            inputs: InputFilter::new(&job.inputs)?,
            outputs: job.outputs.iter().map(|p| p.as_str().to_string()).collect(),
            timeout: job.timeout,
            platform: job.platform.clone(),
            shards: 1,
            risk: None,
            affected_targets: false,
            secrets: Vec::new(),
            junit: None,
        };
        let action = compile(&self.space, &self.trees, root, &step, &self.defaults, None)
            .await?
            .pop()
            .ok_or_else(|| Error::InvalidAction("a regenerate step compiled to nothing".into()))?;
        let (result, _) = self.submit(action, Priority::High, None).await?;
        if !result.status.is_ok() {
            let mut why = result.status.describe();
            if let Some(excerpt) = self.excerpt(&result).await? {
                why.push_str(":\n");
                why.push_str(&excerpt);
            }
            return Ok(Err(why));
        }
        let mut files = BTreeMap::new();
        for path in &job.outputs {
            let Some((_, digest)) = result.outputs.iter().find(|(p, _)| p == path.as_str()) else {
                return Ok(Err(format!("it did not write {path}")));
            };
            let bytes = self
                .space
                .read(digest)
                .await?
                .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
            files.insert(path.clone(), bytes);
        }
        Ok(Ok(files))
    }
}
