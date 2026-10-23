//! `rill checks runs|run|log|artifact`.

use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use rill_mcp::json;
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
    #[arg(long, value_name = "VERSION")]
    pub of_version: Option<String>,
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

fn print_run(ctx: &mut Ctx<'_>, run: &pb::VerifyRun) -> Result<()> {
    ctx.emit(json::verify_run(run), |out| {
        out.line(format!(
            "run {} {} ({}): {}",
            run.id,
            json::run_status(run),
            if run.trunk { "trunk" } else { "pre-land" },
            run.summary
        ))?;
        let mut table = Table::new(&["#", "STEP", "STATE", "MS", "LOGS", "ARTIFACTS"]);
        for s in &run.steps {
            let name = match s.shards {
                0 => s.name.clone(),
                n => format!("{} {}/{n}", s.name, s.shard),
            };
            let mut what = json::step_state(s.state.to_i32()).to_string();
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
    let version = a
        .of_version
        .as_deref()
        .map(|v| {
            v.parse::<rill_types::ChangeVersionHash>()
                .map_err(|_| CliError::usage(format!("invalid version {v:?}")))
        })
        .transpose()?;
    let runs = ctx
        .repo()?
        .checks()
        .runs(version.as_ref(), a.limit, None)
        .await?
        .items;
    ctx.emit(
        j!({ "runs": runs.iter().map(json::verify_run).collect::<Vec<_>>() }),
        |out| {
            let mut table =
                Table::new(&["RUN", "STARTED", "STATUS", "STAGE", "VERSION", "SUMMARY"]);
            for r in &runs {
                table.row(vec![
                    r.id.clone(),
                    when(&Timestamp::from_millis(r.started_at)),
                    json::run_status(r).into(),
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
    let checks = ctx.repo()?.checks();
    if !a.watch {
        let run = checks.run(&a.run).await?;
        return print_run(ctx, &run);
    }
    let mut stream = checks.watch_run(&a.run).await?;
    while let Some(run) = futures::StreamExt::next(&mut stream).await {
        print_run(ctx, &run?)?;
    }
    Ok(())
}

/// Write every byte `read(offset)` returns, from `start`, to `out`.
async fn copy<F, Fut>(start: i64, mut read: F, out: &mut dyn Write) -> Result<()>
where
    F: FnMut(i64) -> Fut,
    Fut: std::future::Future<Output = Result<(bytes::Bytes, u64, u64)>>,
{
    let mut offset = start;
    loop {
        let (data, at, size) = read(offset).await?;
        out.write_all(&data)
            .map_err(|e| CliError::usage(format!("writing: {e}")))?;
        let next = at + data.len() as u64;
        if data.is_empty() || next >= size {
            return Ok(());
        }
        offset = next as i64;
    }
}

/// `rill checks log`.
pub async fn log(ctx: &mut Ctx<'_>, a: LogArgs) -> Result<()> {
    let checks = ctx.repo()?.checks();
    let start = a.tail.map_or(0, |t| -(t as i64));
    let mut stdout = std::io::stdout().lock();
    copy(
        start,
        |offset| {
            let (checks, run) = (checks.clone(), a.run.clone());
            async move {
                let r = checks.read_log(&run, a.step, a.stderr, offset, 0).await?;
                Ok((r.data, r.offset, r.size))
            }
        },
        &mut stdout,
    )
    .await
}

/// `rill checks artifact`.
pub async fn artifact(ctx: &mut Ctx<'_>, a: ArtifactArgs) -> Result<()> {
    let checks = ctx.repo()?.checks();
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
            let (checks, run, path) = (checks.clone(), a.run.clone(), a.path.clone());
            async move {
                let r = checks.read_artifact(&run, a.step, &path, offset, 0).await?;
                Ok((r.data, r.offset, r.size))
            }
        },
        out.as_mut(),
    )
    .await
}
