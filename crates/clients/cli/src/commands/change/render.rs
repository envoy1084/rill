//! Human rendering of new versions and checkout updates.

use rill_sdk::local::UpdateReport;
use rill_sdk::VersionResult;
use rill_types::RepoPathBuf;

use crate::output::{short, Out};

pub(super) fn print_version(
    out: &mut Out<'_>,
    verb: &str,
    r: &VersionResult,
) -> std::io::Result<()> {
    out.line(format!(
        "{verb} change {} version {} ({} file change{})",
        out.id(&r.version.change_id.to_string()),
        out.id(&short(&r.hash)),
        r.version.file_changes.len(),
        if r.version.file_changes.len() == 1 {
            ""
        } else {
            "s"
        },
    ))?;
    let conflicts = r.conflicted_paths();
    if !conflicts.is_empty() {
        out.line(out.bad(&format!(
            "conflicts in: {}",
            conflicts
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))?;
    }
    Ok(())
}

pub(super) fn join_paths(paths: &[RepoPathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn print_report(out: &mut Out<'_>, report: &UpdateReport) -> std::io::Result<()> {
    for p in &report.updated {
        out.line(format!("{} {p}", out.good("U")))?;
    }
    for p in &report.removed {
        out.line(format!("{} {p}", out.bad("D")))?;
    }
    for p in &report.merged {
        out.line(format!("{} {p}", out.warn("M")))?;
    }
    for p in &report.conflicts {
        out.line(format!("{} {p}", out.bad("C")))?;
    }
    Ok(())
}
