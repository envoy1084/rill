//! List filters (AIP-160) built from typed options, with values quoted so
//! no input changes the expression.

/// Terms joined with `AND`.
#[derive(Debug, Clone, Default)]
pub struct FilterBuilder {
    terms: Vec<String>,
}

impl FilterBuilder {
    /// An empty filter: everything matches.
    pub fn new() -> Self {
        FilterBuilder::default()
    }

    /// `field = "value"`.
    pub fn eq(mut self, field: &str, value: impl AsRef<str>) -> Self {
        self.terms
            .push(format!("{field} = {}", quote(value.as_ref())));
        self
    }

    /// `field = "value"` when `value` is given.
    pub fn eq_opt(self, field: &str, value: Option<impl AsRef<str>>) -> Self {
        match value {
            Some(v) => self.eq(field, v),
            None => self,
        }
    }

    /// `(field = "a" OR field = "b")`; nothing when `values` is empty.
    pub fn any_of<S: AsRef<str>>(mut self, field: &str, values: &[S]) -> Self {
        if values.is_empty() {
            return self;
        }
        let alternatives: Vec<String> = values
            .iter()
            .map(|v| format!("{field} = {}", quote(v.as_ref())))
            .collect();
        self.terms.push(format!("({})", alternatives.join(" OR ")));
        self
    }

    /// The filter text; empty when there are no terms.
    pub fn build(self) -> String {
        self.terms.join(" AND ")
    }
}

/// A string literal for a filter.
pub fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_quoted_so_they_cannot_change_the_expression() {
        let filter = FilterBuilder::new()
            .any_of("status", &["open", "landed"])
            .eq("owner", r#"a" OR owner = "b"#)
            .build();
        assert_eq!(
            filter,
            r#"(status = "open" OR status = "landed") AND owner = "a\" OR owner = \"b""#
        );
    }
}
