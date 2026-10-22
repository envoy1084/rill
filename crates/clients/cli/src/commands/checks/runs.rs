//! `rill checks runs|run|log|artifact`.

use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use futures::StreamExt;
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill checks runs`.
#[derive(Args, Debug)]
pub struct RunsArgs {
    /// Only runs of this version (hex).
    #[arg(long)]
    pub version: Option<String>,
    /// At most this many.
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

/// `rill checks run`.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// The run id.
    pub run: String,
    /// Print it again on every change until it ends.
    #[arg(long)]
    pub watch: bool,
}

/// `rill checks log`.
#[derive(Args, Debug)]
pub struct LogArgs {
    /// The run id.
    pub run: String,
    /// The step's index, as `rill checks run` shows it.
    pub step: u32,
    /// Standard error instead of standard output.
    #[arg(long)]
    pub stderr: bool,
    /// Only the last this many bytes.
    #[arg(long)]
    pub tail: Option<u64>,
}

/// `rill checks artifact`.
#[derive(Args, Debug)]
pub struct ArtifactArgs {
    /// The run id.
    pub run: String,
    /// The step's index.
    pub step: u32,
    /// The file's path, as the step declared it.
    pub path: String,
    /// Write it here instead of standard output.
    #[arg(long, short)]
    pub out: Option<PathBuf>,
}

fn state(value: i32) -> &'static str {
    match pb::StepState::try_from(value).unwrap_or(pb::StepState::Unspecified) {
        pb::StepState::Queued => "queued",
        pb::StepState::Running => "running",
        pb::StepState::Passed => "passed",
        pb::StepState::Failed => "failed",
        pb::StepState::TimedOut => "timed out",
        pb::StepState::Error => "error",
        pb::StepState::Unaffected => "unaffected",
        pb::StepState::NotGating => "not gating",
        pb::StepState::Unspecified => "-",
    }
}

fn status(run: &pb::VerifyRun) -> &'static str {
    match run.green {
        None => "running",
        Some(true) => "green",
        Some(false) => "red",
    }
}

fn run_json(run: &pb::VerifyRun) -> serde_json::Value {
    j!({
        "id": run.id,
        "version": hex::encode(&run.version),
        "trunk": run.trunk,
        "status": status(run),
        "summary": run.summary,
        "started_at": run.started_at,
        "finished_at": run.finished_at,
        "steps": run.steps.iter().map(|s| j!({
            "index": s.index,
            "name": s.name,
            "shard": (s.shards > 0).then_some((s.shard, s.shards)),
            "state": state(s.state),
            "exit_code": s.exit_code,
            "error": s.error,
            "cached": s.cached,
            "stdout_size": s.stdout_size,
            "stderr_size": s.stderr_size,
            "artifacts": s.artifacts.iter().map(|a| j!({"path": a.path, "size": a.size})).collect::<Vec<_>>(),
            "worker": s.worker,
            "duration_ms": s.duration_ms,
        })).collect::<Vec<_>>(),
    })
}

fn print_run(ctx: &mut Ctx<'_>, run: &pb::VerifyRun) -> Result<()> {
    ctx.emit(run_json(run), |out| {
        out.line(format!(
            "run {} {} ({}): {}",
            run.id,
            status(run),
            if run.trunk { "trunk" } else { "pre-land" },
            run.summary
        ))?;
        let mut table = Table::new(&["#", "STEP", "STATE", "MS", "LOGS", "ARTIFACTS"]);
        for s in &run.steps {
            let name = match s.shards {
                0 => s.name.clone(),
                n => format!("{} {}/{n}", s.name, s.shard),
            };
            let mut what = state(s.state).to_string();
            if let Some(code) = s.exit_code.filter(|c| *c != 0) {
                what.push_str(&format!(" ({code})"));
            }
            if s.cached {
                what.push_str(" cached");
            }
            table.row(vec![
                s.index.to_string(),
                name,
                what,
                s.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                format!("{}+{}", s.stdout_size, s.stderr_size),
                s.artifacts.len().to_string(),
            ]);
        }
        out.table(table)
    })
}

/// `rill checks runs`.
pub async fn list(ctx: &mut Ctx<'_>, a: RunsArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let version = match &a.version {
        Some(v) => hex::decode(v).map_err(|_| CliError::usage(format!("invalid version {v:?}")))?,
        None => Vec::new(),
    };
    let runs = ctx
        .client()?
        .checks_svc()
        .list_runs(pb::ListRunsRequest {
            repo,
            version: version.into(),
            limit: a.limit,
        })
        .await?
        .into_inner()
        .runs;
    ctx.emit(
        j!({ "runs": runs.iter().map(run_json).collect::<Vec<_>>() }),
        |out| {
            let mut table =
                Table::new(&["RUN", "STARTED", "STATUS", "STAGE", "VERSION", "SUMMARY"]);
            for r in &runs {
                table.row(vec![
                    r.id.clone(),
                    when(&Timestamp::from_millis(r.started_at)),
                    status(r).into(),
                    if r.trunk { "trunk" } else { "pre-land" }.into(),
                    hex::encode(&r.version).chars().take(12).collect(),
                    r.summary.clone(),
                ]);
            }
            out.table(table)
        },
    )
}

/// `rill checks run`.
pub async fn show(ctx: &mut Ctx<'_>, a: RunArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let mut svc = ctx.client()?.checks_svc();
    let request = pb::GetRunRequest { repo, run: a.run };
    if !a.watch {
        let run = svc.get_run(request).await?.into_inner();
        return print_run(ctx, &run);
    }
    let mut stream = svc.watch_run(request).await?.into_inner();
    while let Some(run) = stream.next().await {
        print_run(ctx, &run?)?;
    }
    Ok(())
}

/// Write every byte `read(offset)` returns, from `start`, to `out`.
async fn copy<F, Fut>(start: i64, mut read: F, out: &mut dyn Write) -> Result<()>
where
    F: FnMut(i64) -> Fut,
    Fut: std::future::Future<Output = Result<pb::ReadBytesResponse>>,
{
    let mut offset = start;
    loop {
        let chunk = read(offset).await?;
        out.write_all(&chunk.data)
            .map_err(|e| CliError::usage(format!("writing: {e}")))?;
        let next = chunk.offset + chunk.data.len() as u64;
        if chunk.data.is_empty() || next >= chunk.size {
            return Ok(());
        }
        offset = next as i64;
    }
}

/// `rill checks log`.
pub async fn log(ctx: &mut Ctx<'_>, a: LogArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let svc = ctx.client()?.checks_svc();
    let stream = if a.stderr {
        pb::LogStream::Stderr
    } else {
        pb::LogStream::Stdout
    };
    let start = a.tail.map_or(0, |t| -(t as i64));
    let mut stdout = std::io::stdout().lock();
    copy(
        start,
        |offset| {
            let mut svc = svc.clone();
            let request = pb::ReadLogRequest {
                repo: repo.clone(),
                run: a.run.clone(),
                step: a.step,
                stream: stream as i32,
                offset,
                limit: 0,
            };
            async move { Ok(svc.read_log(request).await?.into_inner()) }
        },
        &mut stdout,
    )
    .await
}

/// `rill checks artifact`.
pub async fn artifact(ctx: &mut Ctx<'_>, a: ArtifactArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let svc = ctx.client()?.checks_svc();
    let mut out: Box<dyn Write> = match &a.out {
        Some(path) => Box::new(
            std::fs::File::create(path)
                .map_err(|e| CliError::usage(format!("{}: {e}", path.display())))?,
        ),
        None => Box::new(std::io::stdout().lock()),
    };
    copy(
        0,
        |offset| {
            let mut svc = svc.clone();
            let request = pb::ReadArtifactRequest {
                repo: repo.clone(),
                run: a.run.clone(),
                step: a.step,
                path: a.path.clone(),
                offset,
                limit: 0,
            };
            async move { Ok(svc.read_artifact(request).await?.into_inner()) }
        },
        out.as_mut(),
    )
    .await
}
