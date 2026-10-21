//! `rill export git`: a one-way mirror of trunk into a git branch.
//!
//! Each trunk position becomes one git commit on the branch, made with
//! `git fast-import`: the files that position changed, the original git
//! author for imported history (else the Rill principal), and trailers
//! naming the trunk position and version. Exports are incremental: the
//! branch's last commit says which position it mirrors.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use clap::{Args, Subcommand};
use rill_git_import::{GIT_AUTHOR_PROVENANCE, GIT_DATE_PROVENANCE};
use rill_sdk::{DiffFormat, DiffOptions, RepoClient, Root};
use rill_types::objects::change::FileState;
use rill_types::{EntryKind, FileKind, RepoPathBuf, TrunkSeq};
use serde_json::json as j;
use tokio::io::AsyncWriteExt;

use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// Trailer naming the trunk position a commit mirrors.
pub const TRUNK_TRAILER: &str = "Rill-Trunk";
/// Trailer naming the mirrored version.
pub const VERSION_TRAILER: &str = "Rill-Version";

/// `rill export <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ExportCmd {
    /// Mirror trunk into a git repository's branch.
    Git(GitExportArgs),
}

/// `rill export git`.
#[derive(Args, Debug)]
pub struct GitExportArgs {
    /// The git repository to write (created if missing).
    pub path: PathBuf,
    /// Branch to mirror trunk onto.
    #[arg(long, default_value = "trunk")]
    pub branch: String,
}

/// Run an export subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ExportCmd) -> Result<()> {
    match cmd {
        ExportCmd::Git(args) => git(ctx, args).await,
    }
}

async fn git(ctx: &mut Ctx<'_>, args: GitExportArgs) -> Result<()> {
    let repo_name = ctx.repo_name()?;
    let repo = ctx.client()?.repo(&repo_name);
    if !args.path.join(".git").exists() && !args.path.join("HEAD").exists() {
        std::fs::create_dir_all(&args.path)?;
        git_cmd(&args.path, &["init", "-q"]).await?;
    }
    let from = mirrored_through(&args.path, &args.branch).await?;
    let head = match repo.trunk_head().await {
        Ok(Some((seq, _))) => seq,
        Ok(None) => TrunkSeq::ZERO,
        Err(e) if e.is_not_found() => TrunkSeq::ZERO,
        Err(e) => return Err(e.into()),
    };
    let mut exported = 0;
    if head > from {
        let mut landings = repo.trunk_range(from, head).await?;
        landings.retain(|l| l.seq > from);
        let mut stream = Vec::new();
        for (i, landing) in landings.iter().enumerate() {
            let continues = (i == 0 && from > TrunkSeq::ZERO).then_some(args.branch.as_str());
            commit(&repo, landing, &args.branch, continues, &mut stream).await?;
            exported += 1;
        }
        stream.extend_from_slice(b"done\n");
        fast_import(&args.path, &stream).await?;
    }
    ctx.emit(
        j!({
            "repo": repo_name,
            "branch": args.branch,
            "exported": exported,
            "through": head.get(),
        }),
        |out| {
            out.line(format!(
                "exported {exported} trunk position(s) to {}; it mirrors trunk@{}",
                args.branch,
                head.get()
            ))
        },
    )
}

/// The trunk position the branch's last commit mirrors; zero for none.
async fn mirrored_through(path: &Path, branch: &str) -> Result<TrunkSeq> {
    let output = tokio::process::Command::new("git")
        .args(["log", "-1", "--format=%B", &format!("refs/heads/{branch}")])
        .current_dir(path)
        .output()
        .await?;
    if !output.status.success() {
        return Ok(TrunkSeq::ZERO);
    }
    let message = String::from_utf8_lossy(&output.stdout);
    let seq = message
        .lines()
        .filter_map(|l| l.strip_prefix(&format!("{TRUNK_TRAILER}: trunk@")))
        .find_map(|n| n.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            CliError::usage(format!(
                "branch {branch} has commits that are not a Rill export; pick another --branch"
            ))
        })?;
    Ok(TrunkSeq(seq))
}

/// Append the fast-import commands for one trunk position to `stream`.
async fn commit(
    repo: &RepoClient,
    landing: &rill_sdk::LandEvent,
    branch: &str,
    continues: Option<&str>,
    stream: &mut Vec<u8>,
) -> Result<()> {
    let cv = repo.change_version(landing.hash).await?;
    let author = cv
        .provenance
        .extra
        .get(GIT_AUTHOR_PROVENANCE)
        .cloned()
        .unwrap_or_else(|| format!("{} <{}>", cv.author.name, email(&cv.author)));
    let authored = cv
        .provenance
        .extra
        .get(GIT_DATE_PROVENANCE)
        .and_then(|d| d.parse::<i64>().ok())
        .unwrap_or_else(|| cv.created_at.millis())
        / 1000;
    let message = format!(
        "{}\n\n{TRUNK_TRAILER}: trunk@{}\n{VERSION_TRAILER}: {}\n",
        cv.message.trim_end(),
        landing.seq.get(),
        landing.hash.to_hex()
    );
    let changes = changes(repo, landing.seq).await?;
    stream.extend_from_slice(format!("commit refs/heads/{branch}\n").as_bytes());
    stream.extend_from_slice(format!("author {author} {authored} +0000\n").as_bytes());
    stream.extend_from_slice(
        format!(
            "committer Rill <rill@localhost> {} +0000\n",
            landing.at.millis() / 1000
        )
        .as_bytes(),
    );
    data(stream, message.as_bytes());
    if let Some(branch) = continues {
        stream.extend_from_slice(format!("from refs/heads/{branch}^0\n").as_bytes());
    }
    if landing.seq.get() == 1 || changes.is_none() {
        stream.extend_from_slice(b"deleteall\n");
    }
    let changes = match changes {
        Some(changes) => changes,
        None => snapshot(repo, &Root::Seq(landing.seq)).await?,
    };
    for (path, after) in changes {
        let path = quote(&path);
        match after {
            None => stream.extend_from_slice(format!("D {path}\n").as_bytes()),
            Some((content, kind)) => {
                let mode = match kind {
                    FileKind::Executable => "100755",
                    FileKind::Symlink => "120000",
                    _ => "100644",
                };
                stream.extend_from_slice(format!("M {mode} inline {path}\n").as_bytes());
                let bytes = repo.get_blob(&content).await?;
                data(stream, &bytes);
            }
        }
    }
    stream.push(b'\n');
    Ok(())
}

/// What trunk position `seq` changed against the one before, as paths and
/// their new state; `None` when that cannot be had from one diff (the
/// first position, or too many files), and a full snapshot is needed.
async fn changes(repo: &RepoClient, seq: TrunkSeq) -> Result<Option<Vec<(String, FileState)>>> {
    if seq.get() == 1 {
        return Ok(None);
    }
    let diff = repo
        .diff_with(
            &Root::Seq(TrunkSeq(seq.get() - 1)),
            &Root::Seq(seq),
            DiffOptions {
                format: DiffFormat::Structured,
                context_lines: 0,
                limit: u32::MAX,
                ..DiffOptions::default()
            },
        )
        .await?;
    if diff.truncated {
        return Ok(None);
    }
    Ok(Some(
        diff.files
            .into_iter()
            .map(|f| (f.path.into_string(), f.after))
            .collect(),
    ))
}

/// Every file at `root`.
async fn snapshot(repo: &RepoClient, root: &Root) -> Result<Vec<(String, FileState)>> {
    let mut files = Vec::new();
    let mut dirs = vec![RepoPathBuf::root()];
    while let Some(dir) = dirs.pop() {
        for (name, entry) in repo.list_dir(root, dir.as_path()).await? {
            let path = dir.join(&name)?;
            match entry {
                EntryKind::File { content, kind } => {
                    files.push((path.into_string(), Some((content, kind))))
                }
                EntryKind::Dir { .. } => dirs.push(path),
            }
        }
    }
    Ok(files)
}

fn data(stream: &mut Vec<u8>, bytes: &[u8]) {
    stream.extend_from_slice(format!("data {}\n", bytes.len()).as_bytes());
    stream.extend_from_slice(bytes);
    stream.push(b'\n');
}

/// A fast-import path, C-quoted.
fn quote(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 2);
    out.push('"');
    for c in path.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn email(principal: &rill_types::Principal) -> String {
    principal
        .identity
        .clone()
        .unwrap_or_else(|| format!("{}@rill", principal.name))
}

async fn fast_import(path: &Path, stream: &[u8]) -> Result<()> {
    let mut child = tokio::process::Command::new("git")
        .args(["fast-import", "--quiet", "--done"])
        .current_dir(path)
        .stdin(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    stdin.write_all(stream).await?;
    drop(stdin);
    let status = child.wait().await?;
    if !status.success() {
        return Err(CliError::conflict(format!(
            "git fast-import failed: {status}"
        )));
    }
    Ok(())
}

async fn git_cmd(path: &Path, args: &[&str]) -> Result<()> {
    let status = tokio::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .await?;
    if !status.success() {
        return Err(CliError::conflict(format!("git {} failed", args.join(" "))));
    }
    Ok(())
}
