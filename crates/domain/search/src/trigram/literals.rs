//! Extracting the trigrams a regex requires from its syntax tree.
//!
//! The index can only prune by literals the pattern must contain. We walk
//! the HIR and turn every literal run into a conjunction of its trigrams,
//! alternations into disjunctions, and anything unconstrained (classes,
//! optional groups, lookarounds) into `All`. The result is sound: a
//! document that matches the regex always satisfies the query. It is not
//! complete, which is fine because the regex is run over every candidate.

use regex_syntax::hir::{Class, Hir, HirKind};
use regex_syntax::ParserBuilder;

use super::extract::fold_byte;
use super::query::TrigramQuery;
use crate::error::{Error, Result};

/// The trigram query a search for `pattern` must satisfy.
///
/// `case_insensitive` must match the index being queried: a folded index
/// stores folded trigrams, so the literals are folded the same way.
///
/// Known imprecision: under case-insensitivity, regex-syntax expands a
/// letter into its full Unicode fold set (`k` also matches the Kelvin sign).
/// Only the ASCII members are considered here, so a document that matches
/// solely through such an exotic variant may be pruned.
pub fn required_trigrams_for_regex(pattern: &str, case_insensitive: bool) -> Result<TrigramQuery> {
    let hir = ParserBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .utf8(false)
        .build()
        .parse(pattern)
        .map_err(|e| Error::InvalidPattern(e.to_string()))?;
    Ok(to_query(&hir, case_insensitive))
}

/// One piece of a concatenation: bytes that must appear contiguously, or
/// an opaque sub-query.
enum Frag {
    Lit(Vec<u8>),
    Query(TrigramQuery),
}

fn to_query(hir: &Hir, ci: bool) -> TrigramQuery {
    let terms = frags(hir, ci)
        .into_iter()
        .map(|f| match f {
            Frag::Lit(bytes) => TrigramQuery::literal(&bytes),
            Frag::Query(q) => q,
        })
        .collect();
    TrigramQuery::and(terms)
}

fn frags(hir: &Hir, ci: bool) -> Vec<Frag> {
    match hir.kind() {
        // Zero-width assertions do not break literal adjacency: if the
        // regex matches, the surrounding bytes are still contiguous.
        HirKind::Empty | HirKind::Look(_) => Vec::new(),
        HirKind::Literal(lit) => {
            let bytes = if ci {
                lit.0.iter().map(|&b| fold_byte(b)).collect()
            } else {
                lit.0.to_vec()
            };
            vec![Frag::Lit(bytes)]
        }
        HirKind::Class(class) => match single_byte(class, ci) {
            Some(b) => vec![Frag::Lit(vec![b])],
            None => vec![Frag::Query(TrigramQuery::All)],
        },
        HirKind::Capture(cap) => frags(&cap.sub, ci),
        HirKind::Repetition(rep) => {
            if rep.min == 0 {
                vec![Frag::Query(TrigramQuery::All)]
            } else {
                vec![Frag::Query(to_query(&rep.sub, ci))]
            }
        }
        HirKind::Concat(items) => {
            let mut out: Vec<Frag> = Vec::new();
            for item in items {
                for frag in frags(item, ci) {
                    match (out.last_mut(), frag) {
                        (Some(Frag::Lit(prev)), Frag::Lit(next)) => prev.extend(next),
                        (_, frag) => out.push(frag),
                    }
                }
            }
            out
        }
        HirKind::Alternation(branches) => vec![Frag::Query(TrigramQuery::or(
            branches.iter().map(|b| to_query(b, ci)).collect(),
        ))],
    }
}

/// The one byte a class stands for, if it is effectively a single literal
/// byte (after folding when `ci`).
fn single_byte(class: &Class, ci: bool) -> Option<u8> {
    let mut members: Vec<u8> = Vec::new();
    match class {
        Class::Bytes(c) => {
            for r in c.ranges() {
                if r.end() - r.start() > 3 {
                    return None;
                }
                members.extend(r.start()..=r.end());
            }
        }
        Class::Unicode(c) => {
            for r in c.ranges() {
                if (r.end() as u32).saturating_sub(r.start() as u32) > 3 {
                    return None;
                }
                for ch in r.start()..=r.end() {
                    if ch.is_ascii() {
                        members.push(ch as u8);
                    } else if !ci {
                        return None;
                    }
                }
            }
        }
    }
    if ci {
        members.iter_mut().for_each(|b| *b = fold_byte(*b));
    }
    members.sort_unstable();
    members.dedup();
    match members.as_slice() {
        [b] => Some(*b),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigram::pack;

    fn tri(s: &str) -> TrigramQuery {
        let b = s.as_bytes();
        TrigramQuery::Trigram(pack(b[0], b[1], b[2]))
    }

    #[test]
    fn requires_literal_after_class_gap() {
        let q = required_trigrams_for_regex(r"fn\s+main", false).unwrap();
        assert_eq!(q, TrigramQuery::and(vec![tri("mai"), tri("ain")]));
    }

    #[test]
    fn concatenates_captures_and_assertions() {
        let q = required_trigrams_for_regex(r"\bfo(o)bar\b", false).unwrap();
        assert_eq!(q, TrigramQuery::literal(b"foobar"));
    }

    #[test]
    fn alternation_is_a_disjunction() {
        let q = required_trigrams_for_regex("hello|world", false).unwrap();
        assert_eq!(
            q,
            TrigramQuery::or(vec![
                TrigramQuery::literal(b"hello"),
                TrigramQuery::literal(b"world")
            ])
        );
        assert!(required_trigrams_for_regex("hello|w", false)
            .unwrap()
            .is_all());
    }

    #[test]
    fn optional_and_short_literals_do_not_constrain() {
        assert!(required_trigrams_for_regex("(abc)?", false)
            .unwrap()
            .is_all());
        assert!(required_trigrams_for_regex("ab", false).unwrap().is_all());
        assert_eq!(
            required_trigrams_for_regex("(abc)+", false).unwrap(),
            TrigramQuery::literal(b"abc")
        );
    }

    #[test]
    fn case_insensitive_folds_to_lowercase() {
        let q = required_trigrams_for_regex("HeLLo", true).unwrap();
        assert_eq!(q, TrigramQuery::literal(b"hello"));
        let q = required_trigrams_for_regex("kay", true).unwrap();
        assert_eq!(q, TrigramQuery::literal(b"kay"));
        assert!(required_trigrams_for_regex("[a-z]bc", true)
            .unwrap()
            .is_all());
    }

    #[test]
    fn invalid_pattern_is_an_error() {
        assert!(matches!(
            required_trigrams_for_regex("(", false),
            Err(Error::InvalidPattern(_))
        ));
    }
}
