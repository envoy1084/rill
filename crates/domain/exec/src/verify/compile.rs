//! Steps as REAPI actions in an execution space.

use std::collections::BTreeMap;

use bytes::Bytes;
use prost::Message;
use rill_core::TreeOps;
use rill_reapi_proto::reapi::{command::EnvironmentVariable, platform::Property};
use rill_reapi_proto::reapi::{Action, Command, Platform};
use rill_types::TreeHash;

use super::plan::Step;
use crate::error::Result;
use crate::remote::{BlobDigest, ExecSpace};

/// One shard of a step, ready to submit.
#[derive(Debug, Clone)]
pub struct CompiledAction {
    pub step: String,
    /// `(index, total)` when the step is sharded.
    pub shard: Option<(u32, u32)>,
    pub action: BlobDigest,
    pub platform: Platform,
}

/// A file Rill adds to a step's input root, relative to its working
/// directory, named to the command by an environment variable.
#[derive(Debug, Clone)]
pub struct ExtraFile {
    pub name: String,
    pub env: String,
    pub bytes: Bytes,
}

/// Store `step`'s actions for `root` in `space`: one per shard, sharing
/// an input root of the files the step reads (and `extra`).
pub async fn compile(
    space: &ExecSpace,
    trees: &TreeOps,
    root: TreeHash,
    step: &Step,
    default_platform: &BTreeMap<String, String>,
    extra: Option<ExtraFile>,
) -> Result<Vec<CompiledAction>> {
    let input_root = space.input_root(trees, root, &step.inputs).await?;
    let mut input_root = space.with_directory(input_root, &step.working_dir).await?;
    let mut base_env = step.env.clone();
    if let Some(extra) = extra {
        let path = step.working_dir.join(&extra.name)?;
        input_root = space.with_file(input_root, &path, extra.bytes).await?;
        base_env.insert(extra.env, extra.name);
    }
    let mut properties = default_platform.clone();
    properties.extend(step.platform.clone());
    let platform = Platform {
        properties: properties
            .into_iter()
            .map(|(name, value)| Property { name, value })
            .collect(),
    };
    let mut outputs = step.outputs.clone();
    outputs.sort_unstable();
    outputs.dedup();

    let mut actions = Vec::with_capacity(step.shards as usize);
    for index in 0..step.shards {
        let shard = (step.shards > 1).then_some((index, step.shards));
        let mut env = base_env.clone();
        if let Some((index, total)) = shard {
            env.insert("TEST_SHARD_INDEX".into(), index.to_string());
            env.insert("TEST_TOTAL_SHARDS".into(), total.to_string());
        }
        let command = Command {
            arguments: step.command.clone(),
            environment_variables: env
                .into_iter()
                .map(|(name, value)| EnvironmentVariable { name, value })
                .collect(),
            output_paths: outputs.clone(),
            working_directory: step.working_dir.as_str().to_string(),
            ..Default::default()
        };
        let command = space.put(Bytes::from(command.encode_to_vec())).await?;
        let action = Action {
            command_digest: Some(command.to_proto()),
            input_root_digest: Some(input_root.to_proto()),
            timeout: Some(prost_types::Duration {
                seconds: step.timeout.as_secs() as i64,
                nanos: step.timeout.subsec_nanos() as i32,
            }),
            platform: Some(platform.clone()),
            ..Default::default()
        };
        actions.push(CompiledAction {
            step: step.name.clone(),
            shard,
            action: space.put(Bytes::from(action.encode_to_vec())).await?,
            platform: platform.clone(),
        });
    }
    Ok(actions)
}
