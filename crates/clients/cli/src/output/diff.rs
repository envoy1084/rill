//! Drawing unified diffs: file and hunk headers, added and removed lines,
//! the changed words within a modified line, and (optionally) syntax
//! colouring of the code, the way `git diff --color-words` and `delta` do.

use std::ops::Range;
use std::sync::OnceLock;

use owo_colors::{DynColors, OwoColorize, Style};
use similar::{ChangeTag, TextDiff};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme as SyntaxTheme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use super::theme::{Role, Theme};

/// Below this similarity a removed and an added line are different lines,
/// not one line edited, and no words are singled out.
const SIMILAR_ENOUGH: f32 = 0.5;

/// The syntax colouring theme used unless `[diff] theme` names another.
pub const DEFAULT_SYNTAX_THEME: &str = "base16-ocean.dark";

/// Backgrounds of syntax-coloured diffs (delta's): whole lines, and the
/// changed words within them.
const ADD_LINE: DynColors = DynColors::Rgb(0x0f, 0x2d, 0x1a);
const DEL_LINE: DynColors = DynColors::Rgb(0x3a, 0x10, 0x15);
const ADD_WORD: DynColors = DynColors::Rgb(0x1e, 0x5a, 0x32);
const DEL_WORD: DynColors = DynColors::Rgb(0x7a, 0x1f, 0x27);

/// How a diff is drawn.
pub struct DiffStyle<'a> {
    /// Colour at all.
    pub color: bool,
    /// The theme's roles.
    pub theme: &'a Theme,
    /// Colour the code by its language.
    pub syntax: bool,
    /// The syntax theme's name.
    pub syntax_theme: &'a str,
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntax_themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// The language of `path`, unless it is plain text.
fn language(path: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map_or(name, |(_, e)| e);
    set.find_syntax_by_extension(ext)
        .or_else(|| set.find_syntax_by_extension(name))
        .filter(|s| s.name != "Plain Text")
}

/// Changed byte ranges of a removed line and of the added line replacing it.
pub type WordChanges = (Vec<Range<usize>>, Vec<Range<usize>>);

/// The changed byte ranges of `old` and `new` when they are one line
/// edited; empty when they are too different to compare word by word.
pub fn changed_words(old: &str, new: &str) -> WordChanges {
    let diff = TextDiff::from_words(old, new);
    if diff.ratio() < SIMILAR_ENOUGH {
        return (Vec::new(), Vec::new());
    }
    let (mut o, mut n) = (0, 0);
    let (mut olds, mut news) = (Vec::new(), Vec::new());
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                o += len;
                n += len;
            }
            ChangeTag::Delete => {
                push_range(&mut olds, o..o + len);
                o += len;
            }
            ChangeTag::Insert => {
                push_range(&mut news, n..n + len);
                n += len;
            }
        }
    }
    (olds, news)
}

/// Add a range, merging it with the previous one when they touch.
fn push_range(ranges: &mut Vec<Range<usize>>, r: Range<usize>) {
    match ranges.last_mut() {
        Some(last) if last.end == r.start => last.end = r.end,
        _ => ranges.push(r),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Add,
    Del,
    Context,
}

/// Draws one file's diff, keeping the syntax state of each side.
struct Painter<'a> {
    style: &'a DiffStyle<'a>,
    old: Option<HighlightLines<'static>>,
    new: Option<HighlightLines<'static>>,
}

impl<'a> Painter<'a> {
    fn new(style: &'a DiffStyle<'a>, path: &str) -> Self {
        let lang = if style.color && style.syntax {
            language(path)
        } else {
            None
        };
        let theme: Option<&'static SyntaxTheme> = lang.and_then(|_| {
            let set = syntax_themes();
            set.themes
                .get(style.syntax_theme)
                .or_else(|| set.themes.get(DEFAULT_SYNTAX_THEME))
        });
        let (old, new) = match (lang, theme) {
            (Some(l), Some(t)) => (
                Some(HighlightLines::new(l, t)),
                Some(HighlightLines::new(l, t)),
            ),
            _ => (None, None),
        };
        Painter { style, old, new }
    }

    fn role(&self, role: Role, text: &str) -> String {
        if self.style.color {
            text.style(self.style.theme.of(role)).to_string()
        } else {
            text.to_string()
        }
    }

    /// One code line: its marker and content, with `emphasis` ranges.
    fn line(&mut self, side: Side, content: &str, emphasis: &[Range<usize>]) -> String {
        let marker = match side {
            Side::Add => "+",
            Side::Del => "-",
            Side::Context => " ",
        };
        if !self.style.color {
            return format!("{marker}{content}");
        }
        let spans = self.highlight(side, content);
        match spans {
            Some(spans) => self.syntax_line(side, marker, content, &spans, emphasis),
            None => self.plain_line(side, marker, content, emphasis),
        }
    }

    fn highlight(&mut self, side: Side, content: &str) -> Option<Vec<(Range<usize>, DynColors)>> {
        let mut with_newline = String::with_capacity(content.len() + 1);
        with_newline.push_str(content);
        with_newline.push('\n');
        let colour =
            |h: &mut Option<HighlightLines<'static>>| -> Option<Vec<(Range<usize>, DynColors)>> {
                let spans = h.as_mut()?.highlight_line(&with_newline, syntaxes()).ok()?;
                let mut at = 0;
                Some(
                    spans
                        .into_iter()
                        .map(|(s, text)| {
                            let end = (at + text.len()).min(content.len());
                            let r = at..end;
                            at += text.len();
                            (
                                r,
                                DynColors::Rgb(s.foreground.r, s.foreground.g, s.foreground.b),
                            )
                        })
                        .filter(|(r, _)| r.start < r.end)
                        .collect(),
                )
            };
        match side {
            Side::Del => colour(&mut self.old),
            Side::Add => colour(&mut self.new),
            Side::Context => {
                let _ = colour(&mut self.old);
                colour(&mut self.new)
            }
        }
    }

    fn syntax_line(
        &self,
        side: Side,
        marker: &str,
        content: &str,
        spans: &[(Range<usize>, DynColors)],
        emphasis: &[Range<usize>],
    ) -> String {
        let (line_bg, word_bg, marker_role) = match side {
            Side::Add => (Some(ADD_LINE), ADD_WORD, Role::DiffAdd),
            Side::Del => (Some(DEL_LINE), DEL_WORD, Role::DiffDel),
            Side::Context => (None, ADD_WORD, Role::Dim),
        };
        let mut out = self.role(marker_role, marker);
        for (range, fg) in spans {
            for (piece, emphasized) in split(range.clone(), emphasis) {
                let mut s = Style::new().color(*fg);
                match (emphasized, line_bg) {
                    (true, _) => s = s.on_color(word_bg),
                    (false, Some(bg)) => s = s.on_color(bg),
                    (false, None) => {}
                }
                out.push_str(&(&content[piece]).style(s).to_string());
            }
        }
        out
    }

    fn plain_line(
        &self,
        side: Side,
        marker: &str,
        content: &str,
        emphasis: &[Range<usize>],
    ) -> String {
        let (line_role, word_role) = match side {
            Side::Add => (Role::DiffAdd, Role::DiffAddWord),
            Side::Del => (Role::DiffDel, Role::DiffDelWord),
            Side::Context => return format!("{marker}{content}"),
        };
        let mut out = self.role(line_role, marker);
        for (piece, emphasized) in split(0..content.len(), emphasis) {
            let role = if emphasized { word_role } else { line_role };
            out.push_str(&self.role(role, &content[piece]));
        }
        out
    }
}

/// `range` cut where `emphasis` starts and ends, each piece marked as
/// emphasized or not.
fn split(range: Range<usize>, emphasis: &[Range<usize>]) -> Vec<(Range<usize>, bool)> {
    let mut cuts = vec![range.start, range.end];
    for e in emphasis {
        for p in [e.start, e.end] {
            if p > range.start && p < range.end {
                cuts.push(p);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| {
            let piece = w[0]..w[1];
            let emphasized = emphasis
                .iter()
                .any(|e| e.start <= piece.start && piece.end <= e.end);
            (piece, emphasized)
        })
        .filter(|(p, _)| p.start < p.end)
        .collect()
}

/// The lines of one file's unified diff, drawn.
pub fn render(style: &DiffStyle<'_>, path: &str, unified: &str) -> Vec<String> {
    let mut painter = Painter::new(style, path);
    let mut out = Vec::new();
    let lines: Vec<&str> = unified.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("diff ") {
            out.push(painter.role(Role::DiffMeta, line));
            i += 1;
        } else if line.starts_with("@@") {
            out.push(hunk_header(&painter, line));
            i += 1;
        } else if line.starts_with('-') || line.starts_with('+') {
            let dels: Vec<&str> = lines[i..]
                .iter()
                .take_while(|l| l.starts_with('-') && !l.starts_with("---"))
                .map(|l| &l[1..])
                .collect();
            let adds: Vec<&str> = lines[i + dels.len()..]
                .iter()
                .take_while(|l| l.starts_with('+') && !l.starts_with("+++"))
                .map(|l| &l[1..])
                .collect();
            let words: Vec<WordChanges> = (0..dels.len().min(adds.len()))
                .map(|k| {
                    if style.color {
                        changed_words(dels[k], adds[k])
                    } else {
                        (Vec::new(), Vec::new())
                    }
                })
                .collect();
            for (k, d) in dels.iter().enumerate() {
                let emph = words.get(k).map(|w| w.0.as_slice()).unwrap_or(&[]);
                out.push(painter.line(Side::Del, d, emph));
            }
            for (k, a) in adds.iter().enumerate() {
                let emph = words.get(k).map(|w| w.1.as_slice()).unwrap_or(&[]);
                out.push(painter.line(Side::Add, a, emph));
            }
            i += dels.len() + adds.len();
            if dels.is_empty() && adds.is_empty() {
                out.push(line.to_string());
                i += 1;
            }
        } else if let Some(rest) = line.strip_prefix(' ') {
            out.push(painter.line(Side::Context, rest, &[]));
            i += 1;
        } else if line.starts_with('\\') {
            out.push(painter.role(Role::Dim, line));
            i += 1;
        } else {
            out.push(line.to_string());
            i += 1;
        }
    }
    out
}

fn hunk_header(painter: &Painter<'_>, line: &str) -> String {
    match line[2..].find("@@") {
        Some(end) => {
            let (head, rest) = line.split_at(end + 4);
            format!("{}{}", painter.role(Role::DiffHunk, head), rest)
        }
        None => painter.role(Role::DiffHunk, line),
    }
}

/// Lines added and removed in a unified diff.
pub fn counts(unified: &str) -> (usize, usize) {
    unified.lines().fold((0, 0), |(a, d), l| {
        if l.starts_with('+') && !l.starts_with("+++") {
            (a + 1, d)
        } else if l.starts_with('-') && !l.starts_with("---") {
            (a, d + 1)
        } else {
            (a, d)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::format::strip_ansi;

    const DIFF: &str =
        "--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@ fn main\n keep\n-let x = 1;\n+let x = 2;\n";

    fn style(theme: &Theme, color: bool) -> DiffStyle<'_> {
        DiffStyle {
            color,
            theme,
            syntax: false,
            syntax_theme: DEFAULT_SYNTAX_THEME,
        }
    }

    #[test]
    fn only_the_changed_word_is_singled_out() {
        let (old, new) = changed_words("let x = 1;", "let x = 2;");
        assert_eq!(old.len(), 1);
        assert_eq!(&"let x = 1;"[old[0].clone()], "1;");
        assert_eq!(&"let x = 2;"[new[0].clone()], "2;");
        let (old, new) = changed_words("alpha beta", "completely different words here");
        assert!(
            old.is_empty() && new.is_empty(),
            "unrelated lines are not compared"
        );
    }

    #[test]
    fn without_colour_the_diff_is_unchanged() {
        let theme = Theme::default();
        assert_eq!(
            render(&style(&theme, false), "a.rs", DIFF).join("\n") + "\n",
            DIFF
        );
    }

    #[test]
    fn coloured_lines_keep_their_text() {
        let theme = Theme::default();
        let lines = render(&style(&theme, true), "a.rs", DIFF);
        assert_eq!(strip_ansi(&lines[4]), "-let x = 1;");
        assert_eq!(strip_ansi(&lines[5]), "+let x = 2;");
        assert!(lines[4].contains("\u{1b}["), "removed line is coloured");
        assert_eq!(counts(DIFF), (1, 1));
    }

    #[test]
    fn syntax_colouring_keeps_the_text_too() {
        let theme = Theme::default();
        let s = DiffStyle {
            syntax: true,
            ..style(&theme, true)
        };
        let lines = render(&s, "a.rs", DIFF);
        assert_eq!(strip_ansi(&lines[5]), "+let x = 2;");
    }
}
