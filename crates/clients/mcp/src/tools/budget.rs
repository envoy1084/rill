//! Response budgets. Every tool that can return a lot takes `max_tokens`
//! and `cursor`: it fills the response until the budget is spent, then
//! says where to continue, so one call never floods the model's context.

use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, string, Props};
use crate::error::{ToolError, ToolResult};

/// Rough size of a token in JSON text; budgets are estimates, not counts.
pub const CHARS_PER_TOKEN: usize = 4;
/// Budget when the call names none.
pub const DEFAULT_TOKENS: u64 = 8_000;
const MIN_TOKENS: u64 = 256;
const MAX_TOKENS: u64 = 200_000;

/// Room left in a response, in characters.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    left: usize,
}

impl Budget {
    /// The call's `max_tokens`, clamped to a sane range.
    pub fn from_args(args: &Args) -> ToolResult<Self> {
        let tokens = args.opt_u64("max_tokens")?.unwrap_or(DEFAULT_TOKENS);
        Ok(Budget::tokens(tokens))
    }

    /// A budget of `tokens`.
    pub fn tokens(tokens: u64) -> Self {
        let tokens = tokens.clamp(MIN_TOKENS, MAX_TOKENS) as usize;
        Budget {
            left: tokens * CHARS_PER_TOKEN,
        }
    }

    /// Spend room for `chars`; false, spending nothing, when they do not fit.
    pub fn spend(&mut self, chars: usize) -> bool {
        if chars > self.left {
            return false;
        }
        self.left -= chars;
        true
    }

    /// True once too little is left for another item to be worth it.
    pub fn is_spent(&self) -> bool {
        self.left < 64
    }

    /// Spend room for `value` as JSON.
    pub fn spend_value(&mut self, value: &Value) -> bool {
        self.spend(value.to_string().len())
    }
}

/// Where a cut-short result continues: an item, and a position (a line)
/// inside it. Encoded as `item.inner`; agents pass it back unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    /// Index of the first item still to return.
    pub item: usize,
    /// Position inside that item.
    pub inner: usize,
}

impl Cursor {
    /// The call's `cursor`, or the start.
    pub fn from_args(args: &Args) -> ToolResult<Self> {
        let Some(text) = args.opt_string("cursor")? else {
            return Ok(Cursor::default());
        };
        let bad = || ToolError::invalid(format!("`cursor` {text:?} is not one this server issued"));
        let (item, inner) = text.split_once('.').ok_or_else(bad)?;
        Ok(Cursor {
            item: item.parse().map_err(|_| bad())?,
            inner: inner.parse().map_err(|_| bad())?,
        })
    }

    /// The wire form.
    pub fn encode(&self) -> String {
        format!("{}.{}", self.item, self.inner)
    }
}

/// Items from `start` while they fit, always at least one so a call makes
/// progress; with the cursor of the first item left out.
pub fn take(items: &[Value], start: usize, budget: &mut Budget) -> (Vec<Value>, Option<Cursor>) {
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate().skip(start) {
        if !budget.spend_value(item) && !out.is_empty() {
            return (out, Some(Cursor { item: i, inner: 0 }));
        }
        out.push(item.clone());
    }
    (out, None)
}

/// How many of `lines` fit, at least one.
pub fn fit_lines(lines: &[&str], budget: &mut Budget) -> usize {
    let mut n = 0;
    for line in lines {
        if !budget.spend(line.len() + 1) && n > 0 {
            break;
        }
        n += 1;
    }
    n
}

/// Mark `out` as cut short at `next`, or complete.
pub fn finish(out: &mut Value, next: Option<Cursor>) {
    out["truncated"] = json!(next.is_some());
    if let Some(next) = next {
        out["next_cursor"] = json!(next.encode());
    }
}

/// The `max_tokens` and `cursor` properties.
pub fn props(p: Props) -> Props {
    p.opt(
        "max_tokens",
        integer("Approximate response budget in tokens (default 8000); larger results are cut short with a next_cursor"),
    )
    .opt(
        "cursor",
        string("next_cursor from a previous call with the same arguments, to continue"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_stops_at_budget_but_always_makes_progress() {
        let items: Vec<Value> = (0..100)
            .map(|i| json!({ "n": i, "pad": "x".repeat(50) }))
            .collect();
        let mut budget = Budget::tokens(MIN_TOKENS);
        let (first, next) = take(&items, 0, &mut budget);
        let next = next.expect("cut short");
        assert_eq!(next.item, first.len());
        assert!(first.len() < items.len());

        let mut tiny = Budget { left: 1 };
        let (one, next) = take(&items, 99, &mut tiny);
        assert_eq!(one.len(), 1);
        assert_eq!(next, None);
    }

    #[test]
    fn cursors_round_trip_and_reject_foreign_text() {
        let c = Cursor {
            item: 3,
            inner: 120,
        };
        let args = Args::new(json!({ "cursor": c.encode() })).unwrap();
        assert_eq!(Cursor::from_args(&args).unwrap(), c);
        let bad = Args::new(json!({ "cursor": "abc" })).unwrap();
        assert!(Cursor::from_args(&bad).is_err());
    }
}
