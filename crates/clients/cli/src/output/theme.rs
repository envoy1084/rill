//! The colour theme: one style per semantic role, so every command colours
//! the same things the same way. Roles can be restyled in `[color]` with
//! git's spec syntax: colours (`red`, `bright-blue`, `208`, `#ff8800`),
//! attributes (`bold`, `dim`, `italic`, `ul`, `reverse`) and a background
//! after `on` (`white on red`).

use std::collections::BTreeMap;

use owo_colors::{AnsiColors, DynColors, Style, XtermColors};

/// What a piece of text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// A path added locally or upstream.
    Added,
    /// A deleted path.
    Removed,
    /// A modified path.
    Modified,
    /// A conflicted path.
    Conflict,
    /// A hash or id.
    Id,
    /// A ref name (`trunk`, a release).
    Ref,
    /// Who made something.
    Author,
    /// When something happened.
    Date,
    /// A section heading.
    Header,
    /// Advice on what to do next.
    Hint,
    /// Something that went well.
    Good,
    /// Something to watch.
    Warning,
    /// Something that failed.
    Error,
    /// Secondary text.
    Dim,
    /// Emphasis.
    Bold,
    /// A diff's file header.
    DiffMeta,
    /// A diff hunk header (`@@ … @@`).
    DiffHunk,
    /// An added diff line.
    DiffAdd,
    /// A removed diff line.
    DiffDel,
    /// The changed words of an added line.
    DiffAddWord,
    /// The changed words of a removed line.
    DiffDelWord,
    /// The working change in the history graph.
    GraphWorking,
    /// An unlanded change in the history graph.
    GraphChange,
    /// A trunk landing in the history graph.
    GraphTrunk,
    /// The history graph's lines.
    GraphLine,
}

impl Role {
    /// Every role, with the name it has in `[color]`.
    pub const ALL: &'static [(Role, &'static str)] = &[
        (Role::Added, "added"),
        (Role::Removed, "removed"),
        (Role::Modified, "modified"),
        (Role::Conflict, "conflict"),
        (Role::Id, "id"),
        (Role::Ref, "ref"),
        (Role::Author, "author"),
        (Role::Date, "date"),
        (Role::Header, "header"),
        (Role::Hint, "hint"),
        (Role::Good, "good"),
        (Role::Warning, "warning"),
        (Role::Error, "error"),
        (Role::Dim, "dim"),
        (Role::Bold, "bold"),
        (Role::DiffMeta, "diff-meta"),
        (Role::DiffHunk, "diff-hunk"),
        (Role::DiffAdd, "diff-add"),
        (Role::DiffDel, "diff-del"),
        (Role::DiffAddWord, "diff-add-word"),
        (Role::DiffDelWord, "diff-del-word"),
        (Role::GraphWorking, "graph-working"),
        (Role::GraphChange, "graph-change"),
        (Role::GraphTrunk, "graph-trunk"),
        (Role::GraphLine, "graph-line"),
    ];

    fn default_style(self) -> Style {
        let s = Style::new();
        match self {
            Role::Added | Role::Good | Role::DiffAdd => s.green(),
            Role::Removed | Role::Error | Role::DiffDel => s.red(),
            Role::Modified | Role::Warning | Role::Id => s.yellow(),
            Role::Conflict => s.red().bold(),
            Role::Ref => s.cyan().bold(),
            Role::Author => s.blue(),
            Role::Date => s.green().dimmed(),
            Role::Header => s.bold(),
            Role::Hint | Role::Dim | Role::GraphLine => s.dimmed(),
            Role::Bold => s.bold(),
            Role::DiffMeta => s.bold(),
            Role::DiffHunk => s.cyan(),
            Role::DiffAddWord => s.green().reversed(),
            Role::DiffDelWord => s.red().reversed(),
            Role::GraphWorking => s.magenta().bold(),
            Role::GraphChange => s.blue().bold(),
            Role::GraphTrunk => s.cyan().bold(),
        }
    }
}

/// A style per role.
#[derive(Debug, Clone)]
pub struct Theme {
    styles: BTreeMap<Role, Style>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            styles: Role::ALL
                .iter()
                .map(|(r, _)| (*r, r.default_style()))
                .collect(),
        }
    }
}

impl Theme {
    /// The default theme with `[color]` overrides applied; unknown roles
    /// and malformed specs are reported and skipped.
    pub fn with_overrides(overrides: &BTreeMap<String, String>) -> Self {
        let mut theme = Theme::default();
        for (name, spec) in overrides {
            let Some((role, _)) = Role::ALL.iter().find(|(_, n)| n == name) else {
                tracing::warn!(role = %name, "unknown colour role");
                continue;
            };
            match parse_spec(spec) {
                Some(style) => {
                    theme.styles.insert(*role, style);
                }
                None => tracing::warn!(role = %name, spec = %spec, "malformed colour"),
            }
        }
        theme
    }

    /// The style of `role`.
    pub fn of(&self, role: Role) -> Style {
        self.styles.get(&role).copied().unwrap_or_default()
    }
}

fn color(word: &str) -> Option<DynColors> {
    let ansi = |c| Some(DynColors::Ansi(c));
    match word {
        "black" => ansi(AnsiColors::Black),
        "red" => ansi(AnsiColors::Red),
        "green" => ansi(AnsiColors::Green),
        "yellow" => ansi(AnsiColors::Yellow),
        "blue" => ansi(AnsiColors::Blue),
        "magenta" => ansi(AnsiColors::Magenta),
        "cyan" => ansi(AnsiColors::Cyan),
        "white" => ansi(AnsiColors::White),
        "bright-black" | "gray" | "grey" => ansi(AnsiColors::BrightBlack),
        "bright-red" => ansi(AnsiColors::BrightRed),
        "bright-green" => ansi(AnsiColors::BrightGreen),
        "bright-yellow" => ansi(AnsiColors::BrightYellow),
        "bright-blue" => ansi(AnsiColors::BrightBlue),
        "bright-magenta" => ansi(AnsiColors::BrightMagenta),
        "bright-cyan" => ansi(AnsiColors::BrightCyan),
        "bright-white" => ansi(AnsiColors::BrightWhite),
        hex if hex.starts_with('#') && hex.len() == 7 => {
            let v = u32::from_str_radix(&hex[1..], 16).ok()?;
            Some(DynColors::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
        }
        n => n
            .parse::<u8>()
            .ok()
            .map(|n| DynColors::Xterm(XtermColors::from(n))),
    }
}

/// Parse a spec such as `yellow bold` or `white on red`.
pub fn parse_spec(spec: &str) -> Option<Style> {
    let mut style = Style::new();
    let mut background = false;
    for word in spec.split_whitespace() {
        let word = word.to_ascii_lowercase();
        style = match word.as_str() {
            "on" => {
                background = true;
                continue;
            }
            "normal" | "plain" => style,
            "bold" => style.bold(),
            "dim" => style.dimmed(),
            "italic" => style.italic(),
            "ul" | "underline" => style.underline(),
            "reverse" => style.reversed(),
            other => {
                let c = color(other)?;
                if background {
                    style.on_color(c)
                } else {
                    style.color(c)
                }
            }
        };
    }
    Some(style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use owo_colors::OwoColorize;

    fn paint(style: Style) -> String {
        "x".style(style).to_string()
    }

    #[test]
    fn specs_parse_like_git() {
        assert_eq!(
            paint(parse_spec("yellow bold").unwrap()),
            paint(Style::new().yellow().bold())
        );
        assert_eq!(
            paint(parse_spec("white on red").unwrap()),
            paint(Style::new().white().on_red())
        );
        assert!(parse_spec("#ff8800 ul").is_some());
        assert!(parse_spec("208").is_some());
        assert!(parse_spec("chartreuse").is_none());
    }

    #[test]
    fn overrides_replace_only_their_role() {
        let theme =
            Theme::with_overrides(&BTreeMap::from([("added".to_string(), "blue".to_string())]));
        assert_eq!(paint(theme.of(Role::Added)), paint(Style::new().blue()));
        assert_eq!(
            paint(theme.of(Role::Removed)),
            paint(Role::Removed.default_style())
        );
    }
}
