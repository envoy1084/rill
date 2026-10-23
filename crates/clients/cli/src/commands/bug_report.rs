//! `rill bug-report`: everything useful for a bug report in one file:
//! versions, platform, the doctor's report, settings (no secrets) and the
//! end of the daemon's log.

use std::path::PathBuf;

use clap::Args;
use serde_json::json as j;

use crate::context::Ctx;
use crate::Result;

/// Lines of the daemon log included.
const LOG_LINES: usize = 200;

/// `rill bug-report`.
#[derive(Args, Debug)]
pub struct BugReportArgs {
    /// Where to write it (default: `rill-bug-report-<time>.md` here).
    #[arg(long, short = 'o')]
    pub out: Option<PathBuf>,
}

/// Run `rill bug-report`.
pub async fn run(ctx: &mut Ctx<'_>, args: BugReportArgs) -> Result<()> {
    let doctor = super::doctor::report(ctx).await?;
    let dir = ctx.daemon_dir()?;
    let log = std::fs::read_to_string(dir.log()).unwrap_or_default();
    let tail: Vec<&str> = log
        .lines()
        .rev()
        .take(LOG_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let settings = ctx.settings();
    let report = format!(
        "# rill bug report\n\n\
         - rill {}\n- platform: {} {}\n- settings: color {:?}, pager {:?}, {} alias(es)\n\n\
         ## What happened\n\n(describe what you did, what you expected and what happened)\n\n\
         ## rill doctor\n\n```json\n{}\n```\n\n## Daemon log (last {} lines)\n\n```\n{}\n```\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        settings.color.ui.as_deref().unwrap_or("auto"),
        settings.core.pager.as_deref().unwrap_or("default"),
        settings.alias.len(),
        serde_json::to_string_pretty(&doctor)?,
        tail.len(),
        tail.join("\n"),
    );
    let path = match args.out {
        Some(p) => p,
        None => {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            ctx.cwd().join(format!("rill-bug-report-{secs}.md"))
        }
    };
    std::fs::write(&path, report)?;
    ctx.emit(j!({ "path": path.display().to_string() }), |out| {
        out.line(format!(
            "wrote {}; attach it to your report",
            path.display()
        ))?;
        out.hint(
            "it holds no tokens, but read it before sharing: it names your repositories and paths",
        )
    })
}
