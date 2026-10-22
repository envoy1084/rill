//! Running one action: stage its inputs, run its command, store what it
//! produced.

use std::path::Path;
use std::time::Duration;

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::{
    Action, ActionResult, Command, ExecuteResponse, ExecutedActionMetadata, OutputFile,
};
use rill_reapi_proto::rpc::Status;
use rill_types::RepoPathBuf;

use super::stage::stage;
use crate::error::{Error, Result};
use crate::remote::timestamps::proto_now as now;
use crate::remote::{BlobDigest, ExecSpace};
use crate::sandbox::{local_path, Invocation, Sandbox};

/// Timeout of an action that sets none; the scheduler normally sets one.
const FALLBACK_TIMEOUT: Duration = Duration::from_secs(600);

/// Run `action` and describe the outcome; failures to run become a
/// status rather than an error, as REAPI reports them.
pub(crate) async fn execute(
    space: &ExecSpace,
    sandbox: &dyn Sandbox,
    worker: &str,
    action: &Action,
    queued: Option<prost_types::Timestamp>,
) -> ExecuteResponse {
    let mut metadata = ExecutedActionMetadata {
        worker: worker.to_string(),
        queued_timestamp: queued,
        worker_start_timestamp: Some(now()),
        ..Default::default()
    };
    match run(space, sandbox, action, &mut metadata).await {
        Ok((mut result, timed_out)) => {
            metadata.worker_completed_timestamp = Some(now());
            result.execution_metadata = Some(metadata);
            ExecuteResponse {
                result: Some(result),
                status: timed_out.then(|| Status {
                    code: tonic::Code::DeadlineExceeded as i32,
                    message: "the action exceeded its timeout".into(),
                    details: vec![],
                }),
                ..Default::default()
            }
        }
        Err(e) => {
            let code = match e {
                Error::MissingBlob(_) => tonic::Code::FailedPrecondition,
                Error::InvalidAction(_) | Error::InvalidDigest(_) | Error::PathEscape(_) => {
                    tonic::Code::InvalidArgument
                }
                _ => tonic::Code::Internal,
            };
            tracing::warn!(error = %e, "local worker could not run an action");
            ExecuteResponse {
                status: Some(Status {
                    code: code as i32,
                    message: e.to_string(),
                    details: vec![],
                }),
                ..Default::default()
            }
        }
    }
}

async fn run(
    space: &ExecSpace,
    sandbox: &dyn Sandbox,
    action: &Action,
    metadata: &mut ExecutedActionMetadata,
) -> Result<(ActionResult, bool)> {
    let command_digest = required(action.command_digest.as_ref(), "command")?;
    let command = space
        .read(&command_digest)
        .await?
        .ok_or_else(|| Error::MissingBlob(command_digest.to_string()))?;
    let command = Command::decode(command)
        .map_err(|e| Error::Codec(format!("command {command_digest}: {e}")))?;
    let root = required(action.input_root_digest.as_ref(), "input root")?;

    let dir = tokio::task::spawn_blocking(tempfile::tempdir).await??;
    metadata.input_fetch_start_timestamp = Some(now());
    stage(space, root, dir.path()).await?;
    metadata.input_fetch_completed_timestamp = Some(now());

    let working_dir = RepoPathBuf::new(&command.working_directory)
        .map_err(|e| Error::InvalidAction(format!("working directory: {e}")))?;
    let cwd = local_path(dir.path(), &working_dir)?;
    let outputs = output_paths(&command)?;
    let parents: Vec<_> = outputs
        .iter()
        .filter_map(|p| local_path(&cwd, p).ok()?.parent().map(Path::to_path_buf))
        .chain(std::iter::once(cwd.clone()))
        .collect();
    tokio::task::spawn_blocking(move || parents.iter().try_for_each(std::fs::create_dir_all))
        .await??;

    let invocation = Invocation {
        command: command.arguments.clone(),
        env: command
            .environment_variables
            .iter()
            .map(|v| (v.name.clone(), v.value.clone()))
            .collect(),
        working_dir,
        timeout: action
            .timeout
            .as_ref()
            .filter(|t| t.seconds > 0 || t.nanos > 0)
            .map(|t| Duration::new(t.seconds.max(0) as u64, t.nanos.max(0) as u32))
            .unwrap_or(FALLBACK_TIMEOUT),
    };
    metadata.execution_start_timestamp = Some(now());
    let raw = sandbox.run(dir.path(), &invocation).await?;
    metadata.execution_completed_timestamp = Some(now());

    metadata.output_upload_start_timestamp = Some(now());
    let mut result = ActionResult {
        exit_code: raw.exit_code,
        stdout_digest: Some(space.put(raw.stdout).await?.to_proto()),
        stderr_digest: Some(space.put(raw.stderr).await?.to_proto()),
        ..Default::default()
    };
    for path in outputs {
        let local = local_path(&cwd, &path)?;
        let read = tokio::task::spawn_blocking(move || read_output(&local)).await??;
        if let Some((bytes, executable)) = read {
            result.output_files.push(OutputFile {
                path: path.as_str().to_string(),
                digest: Some(space.put(bytes).await?.to_proto()),
                is_executable: executable,
                ..Default::default()
            });
        }
    }
    metadata.output_upload_completed_timestamp = Some(now());
    tokio::task::spawn_blocking(move || drop(dir)).await?;
    Ok((result, raw.timed_out))
}

fn required(digest: Option<&rill_reapi_proto::reapi::Digest>, what: &str) -> Result<BlobDigest> {
    BlobDigest::parse(digest.ok_or_else(|| Error::InvalidAction(format!("no {what} digest")))?)
}

/// The paths the command declares as outputs, relative to its working
/// directory.
fn output_paths(command: &Command) -> Result<Vec<RepoPathBuf>> {
    command
        .output_paths
        .iter()
        .map(|p| {
            RepoPathBuf::new(p).map_err(|e| Error::InvalidAction(format!("output {p:?}: {e}")))
        })
        .collect()
}

/// A regular output file's bytes and executable bit; anything else (a
/// directory, a missing path) is not captured.
fn read_output(path: &Path) -> std::io::Result<Option<(Bytes, bool)>> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = {
        let _ = meta;
        false
    };
    Ok(Some((Bytes::from(std::fs::read(path)?), executable)))
}
