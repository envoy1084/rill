//! List filters, a subset of AIP-160: `status = "open" AND (owner = "ann"
//! OR labels:urgent) AND NOT size > 10`.
//!
//! Comparisons are `=`, `!=`, `<`, `<=`, `>`, `>=` and `:` (a list holds
//! the value; a string contains it); `=` on strings takes `*` wildcards.
//! Terms join with `AND` (or whitespace), `OR` and `NOT`/`-`, with
//! parentheses. Values are quoted strings, numbers, `true`/`false`, or bare
//! words read as strings. Each list declares the fields it filters on and
//! their kinds; anything else is refused when the filter is parsed.

mod eval;
mod lex;
mod parse;

use rill_runtime::error::{Error, Result};

/// What kind of value a filterable field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Str,
    Int,
    Bool,
    /// Several strings; only `:` applies.
    List,
}

/// A field's value on one item.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Str(String),
    Int(i64),
    Bool(bool),
    List(Vec<String>),
    /// The item has no value for the field: only `!=` holds.
    Missing,
}

/// An item a filter can be applied to.
pub trait Record {
    /// The value of `field`, one of the schema's fields.
    fn field(&self, field: &str) -> FieldValue;
}

/// A parsed filter, checked against its list's fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    expr: Option<parse::Expr>,
}

impl Filter {
    /// Parse `text` for a list whose filterable fields are `schema`. An
    /// empty filter matches everything.
    pub fn parse(text: &str, schema: &[(&str, Kind)]) -> Result<Self> {
        if text.trim().is_empty() {
            return Ok(Filter { expr: None });
        }
        let tokens = lex::tokens(text).map_err(Error::InvalidFilter)?;
        let expr = parse::parse(&tokens, schema).map_err(Error::InvalidFilter)?;
        Ok(Filter { expr: Some(expr) })
    }

    /// Whether `item` passes.
    pub fn matches(&self, item: &impl Record) -> bool {
        self.expr.as_ref().is_none_or(|e| eval::eval(e, item))
    }

    /// The string values `field` is compared with, for lists that must
    /// look them up (a name the items store as an id).
    pub fn values_of(&self, field: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(expr) = &self.expr {
            collect(expr, field, &mut out);
        }
        out
    }
}

fn collect(expr: &parse::Expr, field: &str, out: &mut Vec<String>) {
    match expr {
        parse::Expr::And(a, b) | parse::Expr::Or(a, b) => {
            collect(a, field, out);
            collect(b, field, out);
        }
        parse::Expr::Not(e) => collect(e, field, out),
        parse::Expr::Compare {
            field: f,
            value: parse::Value::Str(v),
            ..
        } if f == field => out.push(v.clone()),
        parse::Expr::Compare { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Review {
        status: &'static str,
        owner: &'static str,
        size: i64,
        draft: bool,
        labels: Vec<&'static str>,
    }

    impl Record for Review {
        fn field(&self, field: &str) -> FieldValue {
            match field {
                "status" => FieldValue::Str(self.status.into()),
                "owner" => FieldValue::Str(self.owner.into()),
                "size" => FieldValue::Int(self.size),
                "draft" => FieldValue::Bool(self.draft),
                "labels" => FieldValue::List(self.labels.iter().map(|l| l.to_string()).collect()),
                _ => FieldValue::Missing,
            }
        }
    }

    const SCHEMA: &[(&str, Kind)] = &[
        ("status", Kind::Str),
        ("owner", Kind::Str),
        ("size", Kind::Int),
        ("draft", Kind::Bool),
        ("labels", Kind::List),
    ];

    fn review() -> Review {
        Review {
            status: "open",
            owner: "ann",
            size: 12,
            draft: false,
            labels: vec!["urgent", "infra"],
        }
    }

    fn holds(filter: &str) -> bool {
        Filter::parse(filter, SCHEMA).unwrap().matches(&review())
    }

    #[test]
    fn comparisons_combine_with_and_or_not_and_parentheses() {
        assert!(holds(""));
        assert!(holds(r#"status = "open""#));
        assert!(holds("status = open AND owner = ann"));
        assert!(holds("status = open owner = ann"), "whitespace is AND");
        assert!(!holds(r#"status = "open" AND owner = "bob""#));
        assert!(holds(r#"owner = "bob" OR (status = open AND size > 10)"#));
        assert!(holds("NOT draft = true"));
        assert!(holds("-owner = bob"));
        assert!(holds("size >= 12 AND size < 13 AND size != 5"));
        assert!(holds("labels:urgent"));
        assert!(!holds("labels:docs"));
        assert!(holds(r#"owner = "a*""#), "a wildcard");
        assert!(holds("status:pe"), "a substring");
    }

    #[test]
    fn values_compared_with_a_field_are_listed() {
        let filter =
            Filter::parse(r#"owner = ann OR (NOT owner = "bob" AND size > 3)"#, SCHEMA).unwrap();
        assert_eq!(filter.values_of("owner"), ["ann", "bob"]);
        assert!(filter.values_of("status").is_empty());
    }

    #[test]
    fn unknown_fields_wrong_kinds_and_bad_syntax_are_refused() {
        for bad in [
            "colour = red",
            "size = big",
            "draft = 3",
            "labels = urgent",
            "status =",
            "(status = open",
            "status = \"open",
            "status open",
            "AND status = open",
        ] {
            assert!(
                matches!(Filter::parse(bad, SCHEMA), Err(Error::InvalidFilter(_))),
                "{bad}"
            );
        }
    }
}
