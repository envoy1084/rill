//! A diffstat, like `git diff --stat`: each file with a bar of `+` and `-`
//! scaled to the terminal, and a summary line.

use super::theme::{Role, Theme};
use owo_colors::OwoColorize;

/// One file's changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// The path.
    pub path: String,
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
    /// A binary file (no line counts).
    pub binary: bool,
}

/// The diffstat lines for `files`, fitted to `width` columns.
pub fn render(files: &[FileStat], width: usize, color: bool, theme: &Theme) -> Vec<String> {
    let paint = |role: Role, text: &str| {
        if color {
            text.style(theme.of(role)).to_string()
        } else {
            text.to_string()
        }
    };
    let name_width = files
        .iter()
        .map(|f| f.path.chars().count())
        .max()
        .unwrap_or(0);
    let most = files.iter().map(|f| f.added + f.removed).max().unwrap_or(0);
    let count_width = most.to_string().len();
    let room = width.saturating_sub(name_width + count_width + 6).max(10);
    let mut out = Vec::with_capacity(files.len() + 1);
    for f in files {
        let total = f.added + f.removed;
        let bar = if f.binary {
            "Bin".to_string()
        } else {
            let scale = |n: usize| {
                if most <= room {
                    n
                } else {
                    (n * room).div_ceil(most)
                }
            };
            format!(
                "{}{}",
                paint(Role::DiffAdd, &"+".repeat(scale(f.added))),
                paint(Role::DiffDel, &"-".repeat(scale(f.removed)))
            )
        };
        out.push(format!(
            " {:<name_width$} | {:>count_width$} {bar}",
            f.path,
            if f.binary {
                String::new()
            } else {
                total.to_string()
            }
        ));
    }
    let (added, removed) = files
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    out.push(format!(
        " {} changed, {}(+), {}(-)",
        plural(files.len(), "file"),
        plural(added, "insertion"),
        plural(removed, "deletion")
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_scale_to_the_width_and_the_summary_adds_up() {
        let files = vec![
            FileStat {
                path: "src/a.rs".into(),
                added: 300,
                removed: 100,
                binary: false,
            },
            FileStat {
                path: "b".into(),
                added: 1,
                removed: 0,
                binary: false,
            },
            FileStat {
                path: "logo.png".into(),
                added: 0,
                removed: 0,
                binary: true,
            },
        ];
        let lines = render(&files, 60, false, &Theme::default());
        assert!(lines[0].starts_with(" src/a.rs | 400 +"));
        assert!(lines[0].chars().count() <= 60, "{}", lines[0]);
        assert_eq!(lines[1], " b        |   1 +");
        assert!(lines[2].ends_with("Bin"));
        assert_eq!(
            lines[3],
            " 3 files changed, 301 insertions(+), 100 deletions(-)"
        );
    }
}
