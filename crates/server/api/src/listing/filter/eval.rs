//! Applying an expression to one item.

use super::lex::Op;
use super::parse::{Expr, Value};
use super::{FieldValue, Record};

pub(super) fn eval(expr: &Expr, item: &impl Record) -> bool {
    match expr {
        Expr::And(a, b) => eval(a, item) && eval(b, item),
        Expr::Or(a, b) => eval(a, item) || eval(b, item),
        Expr::Not(e) => !eval(e, item),
        Expr::Compare { field, op, value } => compare(&item.field(field), *op, value),
    }
}

fn compare(have: &FieldValue, op: Op, want: &Value) -> bool {
    use std::cmp::Ordering;
    let ordering = match (have, want) {
        (FieldValue::List(items), Value::Str(s)) => return items.iter().any(|i| i == s),
        (FieldValue::Str(h), Value::Str(w)) if op == Op::Has => return h.contains(w.as_str()),
        (FieldValue::Str(h), Value::Str(w)) if matches!(op, Op::Eq | Op::Ne) => {
            return wildcard(w, h) == (op == Op::Eq)
        }
        (FieldValue::Str(h), Value::Str(w)) => h.as_str().cmp(w.as_str()),
        (FieldValue::Int(h), Value::Int(w)) => h.cmp(w),
        (FieldValue::Bool(h), Value::Bool(w)) => h.cmp(w),
        _ => return op == Op::Ne,
    };
    match op {
        Op::Eq | Op::Has => ordering == Ordering::Equal,
        Op::Ne => ordering != Ordering::Equal,
        Op::Lt => ordering == Ordering::Less,
        Op::Le => ordering != Ordering::Greater,
        Op::Gt => ordering == Ordering::Greater,
        Op::Ge => ordering != Ordering::Less,
    }
}

/// Whether `text` matches `pattern`, where `*` matches any run.
fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    if !text.ends_with(last) {
        return false;
    }
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}
