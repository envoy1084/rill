//! Human and JSON output.
//!
//! Every command produces one JSON value (via [`rill_mcp::json`]) and a
//! human rendering of the same data. With `--json` the value is printed
//! as-is; otherwise the human rendering goes through [`Out`], which owns
//! the writer and the colour decision.

mod format;
mod table;

pub use format::{short, when, yes_no};
pub use table::Table;

use std::io::Write;

use owo_colors::{OwoColorize, Style};

/// The writer commands print to, with colour support.
pub struct Out<'w> {
    writer: &'w mut dyn Write,
    color: bool,
}

impl<'w> Out<'w> {
    /// Wrap a writer; `color` enables ANSI styling.
    pub fn new(writer: &'w mut dyn Write, color: bool) -> Self {
        Out { writer, color }
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
        self.styled(text, Style::new().bold())
    }

    /// Dimmed text.
    pub fn dim(&self, text: &str) -> String {
        self.styled(text, Style::new().dimmed())
    }

    /// Green text.
    pub fn good(&self, text: &str) -> String {
        self.styled(text, Style::new().green())
    }

    /// Red text.
    pub fn bad(&self, text: &str) -> String {
        self.styled(text, Style::new().red())
    }

    /// Yellow text.
    pub fn warn(&self, text: &str) -> String {
        self.styled(text, Style::new().yellow())
    }

    /// Cyan text, used for hashes and ids.
    pub fn id(&self, text: &str) -> String {
        self.styled(text, Style::new().cyan())
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
