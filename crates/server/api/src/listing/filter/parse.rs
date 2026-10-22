//! Tokens into an expression, checked against the list's fields.

use super::lex::{Op, Token};
use super::Kind;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare { field: String, op: Op, value: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    schema: &'a [(&'a str, Kind)],
}

/// The expression `tokens` spell, or why they do not.
pub(super) fn parse(tokens: &[Token], schema: &[(&str, Kind)]) -> Result<Expr, String> {
    let mut p = Parser {
        tokens,
        at: 0,
        schema,
    };
    let expr = p.or()?;
    match p.peek() {
        None => Ok(expr),
        Some(t) => Err(format!("unexpected {t:?}")),
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.peek() == Some(&Token::Or) {
            self.at += 1;
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            match self.peek() {
                Some(Token::And) => self.at += 1,
                Some(Token::Ident(_) | Token::Not | Token::Open) => {}
                _ => return Ok(left),
            }
            left = Expr::And(Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Not) => Ok(Expr::Not(Box::new(self.unary()?))),
            Some(Token::Open) => {
                let inner = self.or()?;
                match self.next() {
                    Some(Token::Close) => Ok(inner),
                    _ => Err("missing )".into()),
                }
            }
            Some(Token::Ident(field)) => self.comparison(field),
            Some(t) => Err(format!("expected a field, found {t:?}")),
            None => Err("expected a field".into()),
        }
    }

    fn comparison(&mut self, field: String) -> Result<Expr, String> {
        let kind = self
            .schema
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, kind)| *kind)
            .ok_or_else(|| format!("unknown field {field:?}"))?;
        let op = match self.next() {
            Some(Token::Op(op)) => op,
            _ => return Err(format!("expected a comparison after {field:?}")),
        };
        let value = match (kind, self.next()) {
            (Kind::Str | Kind::List, Some(Token::Str(s) | Token::Ident(s))) => Value::Str(s),
            (Kind::Str | Kind::List, Some(Token::Num(n))) => Value::Str(n.to_string()),
            (Kind::Int, Some(Token::Num(n))) => Value::Int(n),
            (Kind::Bool, Some(Token::Ident(w))) if w == "true" || w == "false" => {
                Value::Bool(w == "true")
            }
            (_, None) => return Err(format!("expected a value for {field:?}")),
            (_, Some(t)) => return Err(format!("{t:?} is not a {kind:?} value for {field:?}")),
        };
        let fits = match kind {
            Kind::List => op == Op::Has,
            Kind::Bool => matches!(op, Op::Eq | Op::Ne),
            Kind::Str | Kind::Int => true,
        };
        if !fits {
            return Err(format!("{op:?} does not apply to {field:?}"));
        }
        Ok(Expr::Compare { field, op, value })
    }
}
