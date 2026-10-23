//! Printing a set of file diffs the way `git diff` does: a header per
//! file, then its drawn hunks; or a diffstat; or just the names.

use super::diff::{self, DiffStyle};
use super::stat::{self, FileStat};
use super::theme::Role;
use super::writer::Out;

/// How a file changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// New on the target side.
    Added,
    /// Gone on the target side.
    Deleted,
    /// Changed.
    Modified,
    /// An unresolved conflict on the target side.
    Conflict,
}

/// One file's diff to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    /// The path.
    pub path: String,
    /// How it changed.
    pub status: FileStatus,
    /// Binary on either side (no hunks).
    pub binary: bool,
    /// Unified diff text; empty for binary files.
    pub unified: String,
}

/// What to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatchMode {
    /// Headers and hunks.
    #[default]
    Full,
    /// A diffstat.
    Stat,
    /// Paths with a status letter.
    NameStatus,
}

impl Out<'_> {
    /// Print `files` in `mode`.
    pub fn patch(&mut self, files: &[PatchFile], mode: PatchMode) -> std::io::Result<()> {
        match mode {
            PatchMode::Stat => {
                let stats: Vec<FileStat> = files
                    .iter()
                    .map(|f| {
                        let (added, removed) = diff::counts(&f.unified);
                        FileStat {
                            path: f.path.clone(),
                            added,
                            removed,
                            binary: f.binary,
                        }
                    })
                    .collect();
                let width = self.width();
                for line in stat::render(&stats, width, self.color(), self.theme()) {
                    self.line(line)?;
                }
                Ok(())
            }
            PatchMode::NameStatus => {
                for f in files {
                    let (letter, role) = status_letter(f.status);
                    let painted = self.paint(role, letter);
                    self.line(format!("{painted}\t{}", f.path))?;
                }
                Ok(())
            }
            PatchMode::Full => {
                for f in files {
                    self.file_patch(f)?;
                }
                Ok(())
            }
        }
    }

    fn file_patch(&mut self, f: &PatchFile) -> std::io::Result<()> {
        let header = format!("diff --rill a/{0} b/{0}", f.path);
        let painted = self.paint(Role::DiffMeta, &header);
        self.line(painted)?;
        let note = match f.status {
            FileStatus::Added => Some("new file"),
            FileStatus::Deleted => Some("deleted file"),
            FileStatus::Conflict => Some("unresolved conflict"),
            FileStatus::Modified => None,
        };
        if let Some(note) = note {
            let painted = self.paint(Role::DiffMeta, note);
            self.line(painted)?;
        }
        if f.binary {
            let painted = self.paint(Role::Dim, "Binary files differ");
            return self.line(painted);
        }
        let style = DiffStyle {
            color: self.color(),
            theme: self.theme(),
            syntax: self.syntax(),
            syntax_theme: self.syntax_theme(),
        };
        let lines = diff::render(&style, &f.path, &f.unified);
        for line in lines {
            self.line(line)?;
        }
        Ok(())
    }
}

/// The letter `--name-status` prints, and its role.
pub fn status_letter(status: FileStatus) -> (&'static str, Role) {
    match status {
        FileStatus::Added => ("A", Role::Added),
        FileStatus::Deleted => ("D", Role::Removed),
        FileStatus::Modified => ("M", Role::Modified),
        FileStatus::Conflict => ("C", Role::Conflict),
    }
}
