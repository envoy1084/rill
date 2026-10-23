//! `rill blame`: who last changed each line, coloured by age (recent
//! landings bright, old ones dim); a run of lines from one version names it
//! once.

use clap::Args;
use rill_mcp::json;
use rill_types::RepoPathBuf;
use serde_json::json as j;

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::output::{short, Role};
use crate::Result;

/// `rill blame`.
#[derive(Args, Debug)]
pub struct BlameArgs {
    /// File path.
    pub path: String,
    /// Root to blame at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// The role of a line last changed at `seq`, given the newest and oldest
/// positions in the file: the newest tenth stands out, the oldest half is
/// dimmed.
pub fn age_role(seq: Option<u64>, oldest: u64, newest: u64) -> Role {
    let Some(seq) = seq else {
        return Role::Warning;
    };
    if newest <= oldest {
        return Role::Bold;
    }
    let age = (newest - seq) as f64 / (newest - oldest) as f64;
    if age <= 0.1 {
        Role::Good
    } else if age >= 0.5 {
        Role::Dim
    } else {
        Role::Author
    }
}

/// Run `rill blame`.
pub async fn blame(ctx: &mut Ctx<'_>, args: BlameArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let path = RepoPathBuf::new(&args.path)?;
    let hash = repo.resolve_root(&root).await?;
    let lines = repo.blame(hash, path.as_path()).await?;
    ctx.emit(
        j!({
            "path": path.as_str(),
            "version": hash.to_hex(),
            "lines": lines.iter().map(json::blame_line).collect::<Vec<_>>(),
        }),
        |out| {
            let seqs: Vec<u64> = lines
                .iter()
                .filter_map(|l| l.trunk_seq.map(|s| s.get()))
                .collect();
            let oldest = seqs.iter().copied().min().unwrap_or(0);
            let newest = seqs.iter().copied().max().unwrap_or(0);
            let num_width = lines.len().to_string().len();
            let author_width = lines
                .iter()
                .map(|l| l.author.to_string().chars().count())
                .max()
                .unwrap_or(0)
                .min(16);
            let mut previous = None;
            for l in &lines {
                let role = age_role(l.trunk_seq.map(|s| s.get()), oldest, newest);
                let annotation = if previous == Some(l.version) {
                    " ".repeat(12 + 1 + author_width + 1 + 7)
                } else {
                    let seq = l
                        .trunk_seq
                        .map(|s| format!("@{}", s.get()))
                        .unwrap_or_else(|| "local".into());
                    let mut author = l.author.to_string();
                    author.truncate(author_width);
                    format!("{} {author:<author_width$} {seq:>7}", short(&l.version))
                };
                previous = Some(l.version);
                out.line(format!(
                    "{} {} {}",
                    out.paint(role, &annotation),
                    out.paint(Role::Dim, &format!("{:>num_width$} │", l.line_no)),
                    String::from_utf8_lossy(&l.text).trim_end_matches(['\n', '\r']),
                ))?;
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_lines_stand_out_and_old_ones_fade() {
        assert_eq!(age_role(Some(100), 0, 100), Role::Good);
        assert_eq!(age_role(Some(70), 0, 100), Role::Author);
        assert_eq!(age_role(Some(10), 0, 100), Role::Dim);
        assert_eq!(age_role(None, 0, 100), Role::Warning);
        assert_eq!(age_role(Some(5), 5, 5), Role::Bold);
    }
}
