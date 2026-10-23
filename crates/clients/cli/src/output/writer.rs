//! The styled writer every command prints its human rendering through.

use std::io::Write;

use std::sync::Arc;

use owo_colors::{OwoColorize, Style};

use super::table::Table;
use super::theme::{Role, Theme};

/// Columns assumed when the terminal's width is unknown.
const DEFAULT_WIDTH: usize = 100;

/// The writer commands print to, with colour support.
pub struct Out<'w> {
    writer: &'w mut dyn Write,
    color: bool,
    theme: Arc<Theme>,
    syntax: bool,
    syntax_theme: String,
}

impl<'w> Out<'w> {
    /// Wrap a writer; `color` enables ANSI styling with the default theme.
    pub fn new(writer: &'w mut dyn Write, color: bool) -> Self {
        Out {
            writer,
            color,
            theme: Arc::new(Theme::default()),
            syntax: false,
            syntax_theme: super::diff::DEFAULT_SYNTAX_THEME.into(),
        }
    }

    /// Colour code in diffs by language, with the named syntax theme.
    pub fn with_syntax(mut self, on: bool, theme: Option<String>) -> Self {
        self.syntax = on;
        if let Some(t) = theme {
            self.syntax_theme = t;
        }
        self
    }

    /// The colour theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Whether diffs colour code by language.
    pub fn syntax(&self) -> bool {
        self.syntax
    }

    /// The syntax colouring theme's name.
    pub fn syntax_theme(&self) -> &str {
        &self.syntax_theme
    }

    /// Use `theme` for every role.
    pub fn with_theme(mut self, theme: Arc<Theme>) -> Self {
        self.theme = theme;
        self
    }

    /// Text in `role`'s style (plain without colour).
    pub fn paint(&self, role: Role, text: &str) -> String {
        self.styled(text, self.theme.of(role))
    }

    /// The terminal's width: `$COLUMNS`, else the terminal's, else 100.
    pub fn width(&self) -> usize {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|c| c.parse().ok())
            .or_else(|| terminal_size::terminal_size().map(|(w, _)| w.0 as usize))
            .unwrap_or(DEFAULT_WIDTH)
    }

    /// True when ANSI colour is on.
    pub fn color(&self) -> bool {
        self.color
    }

    /// Write a line.
    pub fn line(&mut self, text: impl AsRef<str>) -> std::io::Result<()> {
        self.writer.write_all(text.as_ref().as_bytes())?;
        self.writer.write_all(b"\n")
    }

    /// Write text without a trailing newline.
    pub fn raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(bytes)
    }

    /// Write an empty line.
    pub fn blank(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"\n")
    }

    /// Write aligned `key: value` pairs.
    pub fn kv(&mut self, pairs: &[(&str, String)]) -> std::io::Result<()> {
        let width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in pairs {
            let key = format!("{k:>width$}");
            let key = self.styled(&key, Style::new().dimmed());
            self.line(format!("{key}  {v}"))?;
        }
        Ok(())
    }

    /// Render a table.
    pub fn table(&mut self, table: Table) -> std::io::Result<()> {
        let text = table.render(self.color);
        if !text.is_empty() {
            self.line(text)?;
        }
        Ok(())
    }

    /// Flush the underlying writer.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    /// Apply a style when colour is on.
    pub fn styled(&self, text: &str, style: Style) -> String {
        if self.color {
            text.style(style).to_string()
        } else {
            text.to_string()
        }
    }

    /// Bold text.
    pub fn bold(&self, text: &str) -> String {
        self.paint(Role::Bold, text)
    }

    /// Dimmed text.
    pub fn dim(&self, text: &str) -> String {
        self.paint(Role::Dim, text)
    }

    /// Something that went well.
    pub fn good(&self, text: &str) -> String {
        self.paint(Role::Good, text)
    }

    /// Something that failed.
    pub fn bad(&self, text: &str) -> String {
        self.paint(Role::Error, text)
    }

    /// Something to watch.
    pub fn warn(&self, text: &str) -> String {
        self.paint(Role::Warning, text)
    }

    /// A hash or id.
    pub fn id(&self, text: &str) -> String {
        self.paint(Role::Id, text)
    }

    /// A `hint:` line after an error or a state, like git's.
    pub fn hint(&mut self, text: &str) -> std::io::Result<()> {
        let line = format!("hint: {text}");
        let painted = self.paint(Role::Hint, &line);
        self.line(painted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_aligns_and_styles_only_with_color() {
        let mut buf = Vec::new();
        {
            let mut out = Out::new(&mut buf, false);
            out.kv(&[("a", "1".into()), ("long", "2".into())]).unwrap();
        }
        assert_eq!(String::from_utf8(buf).unwrap(), "   a  1\nlong  2\n");
        let mut colored = Vec::new();
        {
            let mut out = Out::new(&mut colored, true);
            out.line(out.bold("x")).unwrap();
        }
        assert!(String::from_utf8(colored).unwrap().contains("\x1b["));
    }
}
