//! A thin wrapper over `comfy-table` with the one style `rill` uses.

use comfy_table::{presets, Attribute, Cell, ContentArrangement};

/// A table under construction.
#[derive(Debug, Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// Start a table with these column headers.
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    /// Append a row.
    pub fn row(&mut self, cells: Vec<String>) -> &mut Self {
        self.rows.push(cells);
        self
    }

    /// True when no rows were added.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Render without a trailing newline, fitted to `width` columns. An
    /// empty table renders as an empty string so callers can print a
    /// "nothing here" line instead.
    pub fn render(&self, color: bool, width: usize) -> String {
        if self.rows.is_empty() {
            return String::new();
        }
        let mut table = comfy_table::Table::new();
        table
            .load_style(presets::NOTHING)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(width.min(u16::MAX as usize) as u16);
        if color {
            table.enforce_styling();
        }
        table.set_header(self.headers.iter().map(|h| {
            let cell = Cell::new(h);
            if color {
                cell.add_attribute(Attribute::Bold)
            } else {
                cell
            }
        }));
        for row in &self.rows {
            table.add_row(row.iter().map(Cell::new));
        }
        table.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_rows_and_nothing_when_empty() {
        assert_eq!(Table::new(&["a"]).render(false, 100), "");
        let mut t = Table::new(&["name", "n"]);
        t.row(vec!["x".into(), "1".into()]);
        let text = t.render(false, 100);
        assert!(text.contains("name"));
        assert!(text.contains('x'));
    }
}
