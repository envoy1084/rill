//! The trigram query a regex requires, after Russ Cox's codesearch
//! (`index/regexp.go`, BSD-3-Clause), ported to `regex-syntax`'s HIR.
//!
//! Each sub-expression is summarised by what any match of it must look
//! like: whether it can match the empty string, the exact set of strings
//! it matches (when small), sets of strings every match starts and ends
//! with, and a trigram query every match satisfies. Concatenation crosses
//! exact sets, so `(abc|def)ghi` requires `abcghi` or `defghi` rather than
//! just `ghi`; sets that grow too large are folded into the query and
//! replaced by their prefixes and suffixes. The result is sound: a
//! document that matches the regex always satisfies the query. The regex
//! itself is still run over every candidate.

use std::collections::BTreeSet;

use regex_syntax::hir::{Class, Hir, HirKind};
use regex_syntax::ParserBuilder;

use super::extract::fold_byte;
use super::query::TrigramQuery;
use crate::error::{Error, Result};

/// Largest exact or affix set kept before it is folded into the query.
const MAX_SET: usize = 32;
/// Longest exact string kept; longer ones become prefix and suffix.
const MAX_EXACT: usize = 64;
/// Most members of a character class enumerated into an exact set.
const MAX_CLASS: usize = 8;

type Set = BTreeSet<Vec<u8>>;

/// What every match of a sub-expression looks like.
#[derive(Debug, Clone)]
struct Info {
    can_empty: bool,
    /// Every string the expression matches, when known and small.
    exact: Option<Set>,
    /// Every match starts with one of these (meaningful when `exact` is
    /// `None`); `{""}` means nothing is known.
    prefix: Set,
    /// Every match ends with one of these.
    suffix: Set,
    /// A query every match satisfies.
    query: TrigramQuery,
}

fn set(items: impl IntoIterator<Item = Vec<u8>>) -> Set {
    items.into_iter().collect()
}

fn unknown() -> Set {
    set([Vec::new()])
}

/// The trigram query of "contains one of `strings`".
fn or_query(strings: &Set) -> TrigramQuery {
    TrigramQuery::or(strings.iter().map(|s| TrigramQuery::literal(s)).collect())
}

fn cross(a: &Set, b: &Set) -> Set {
    let mut out = Set::new();
    for x in a {
        for y in b {
            let mut s = x.clone();
            s.extend_from_slice(y);
            out.insert(s);
        }
    }
    out
}

impl Info {
    fn exact(strings: Set) -> Info {
        Info {
            can_empty: strings.iter().any(Vec::is_empty),
            exact: Some(strings),
            prefix: Set::new(),
            suffix: Set::new(),
            query: TrigramQuery::All,
        }
    }

    fn any() -> Info {
        Info {
            can_empty: true,
            exact: None,
            prefix: unknown(),
            suffix: unknown(),
            query: TrigramQuery::All,
        }
    }

    /// Something that matches at least one unknown byte.
    fn any_char() -> Info {
        Info {
            can_empty: false,
            ..Info::any()
        }
    }

    /// The prefix set, deriving it from the exact set when known.
    fn prefixes(&self) -> Set {
        self.exact.clone().unwrap_or_else(|| self.prefix.clone())
    }

    fn suffixes(&self) -> Set {
        self.exact.clone().unwrap_or_else(|| self.suffix.clone())
    }

    /// Fold an oversized exact set into the query, and trim affix sets.
    fn simplify(mut self, force: bool) -> Info {
        if let Some(exact) = &self.exact {
            let too_big = exact.len() > MAX_SET || exact.iter().any(|s| s.len() > MAX_EXACT);
            if force || too_big {
                let exact = self.exact.take().expect("checked above");
                self.query = TrigramQuery::and(vec![self.query, or_query(&exact)]);
                self.prefix = exact.clone();
                self.suffix = exact;
            }
        }
        if self.exact.is_none() {
            self.prefix = trim(std::mem::take(&mut self.prefix), &mut self.query, true);
            self.suffix = trim(std::mem::take(&mut self.suffix), &mut self.query, false);
        }
        self
    }
}

/// Keep an affix set small: record what it implies in `query`, then cut
/// its strings to three bytes (from the start for prefixes, the end for
/// suffixes) until it fits, or forget it.
fn trim(affixes: Set, query: &mut TrigramQuery, prefix: bool) -> Set {
    if affixes.len() <= MAX_SET && affixes.iter().all(|s| s.len() <= MAX_EXACT) {
        return affixes;
    }
    *query = TrigramQuery::and(vec![
        std::mem::replace(query, TrigramQuery::All),
        or_query(&affixes),
    ]);
    let cut: Set = affixes
        .into_iter()
        .map(|s| {
            if s.len() <= 3 {
                s
            } else if prefix {
                s[..3].to_vec()
            } else {
                s[s.len() - 3..].to_vec()
            }
        })
        .collect();
    if cut.len() <= MAX_SET {
        cut
    } else {
        unknown()
    }
}

fn concat(x: Info, y: Info) -> Info {
    let mut out = Info {
        can_empty: x.can_empty && y.can_empty,
        exact: None,
        prefix: Set::new(),
        suffix: Set::new(),
        query: TrigramQuery::and(vec![x.query.clone(), y.query.clone()]),
    };
    match (&x.exact, &y.exact) {
        (Some(a), Some(b)) if a.len() * b.len() <= MAX_SET => {
            out.exact = Some(cross(a, b));
        }
        _ => {
            out.prefix = match &x.exact {
                Some(a) => cross(a, &y.prefixes()),
                None if x.can_empty => x.prefix.union(&y.prefixes()).cloned().collect(),
                None => x.prefix.clone(),
            };
            out.suffix = match &y.exact {
                Some(b) => cross(&x.suffixes(), b),
                None if y.can_empty => y.suffix.union(&x.suffixes()).cloned().collect(),
                None => y.suffix.clone(),
            };
            // Exact sets that were not crossed still constrain.
            if out.exact.is_none() {
                for e in [&x.exact, &y.exact].into_iter().flatten() {
                    out.query = TrigramQuery::and(vec![out.query, or_query(e)]);
                }
            }
        }
    }
    out.simplify(false)
}

fn alternate(x: Info, y: Info) -> Info {
    let can_empty = x.can_empty || y.can_empty;
    if let (Some(a), Some(b)) = (&x.exact, &y.exact) {
        return Info {
            can_empty,
            ..Info::exact(a.union(b).cloned().collect())
        }
        .simplify(false);
    }
    let query = |i: &Info| match &i.exact {
        Some(e) => or_query(e),
        None => i.query.clone(),
    };
    Info {
        can_empty,
        exact: None,
        prefix: x.prefixes().union(&y.prefixes()).cloned().collect(),
        suffix: x.suffixes().union(&y.suffixes()).cloned().collect(),
        query: TrigramQuery::or(vec![query(&x), query(&y)]),
    }
    .simplify(false)
}

/// The members of a small class as strings (UTF-8 for Unicode classes),
/// folded when `ci`; `None` when the class is large.
fn class_strings(class: &Class, ci: bool) -> Option<Set> {
    let mut out = Set::new();
    match class {
        Class::Bytes(c) => {
            for r in c.ranges() {
                if usize::from(r.end() - r.start()) >= MAX_CLASS {
                    return None;
                }
                for b in r.start()..=r.end() {
                    out.insert(vec![if ci { fold_byte(b) } else { b }]);
                }
            }
        }
        Class::Unicode(c) => {
            for r in c.ranges() {
                if (r.end() as u32 - r.start() as u32) as usize >= MAX_CLASS {
                    return None;
                }
                for ch in r.start()..=r.end() {
                    let mut buf = [0u8; 4];
                    let mut s = ch.encode_utf8(&mut buf).as_bytes().to_vec();
                    if ci {
                        s.iter_mut().for_each(|b| *b = fold_byte(*b));
                    }
                    out.insert(s);
                }
            }
        }
    }
    (out.len() <= MAX_CLASS).then_some(out)
}

fn analyze(hir: &Hir, ci: bool) -> Info {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => Info::exact(set([Vec::new()])),
        HirKind::Literal(lit) => {
            let bytes = if ci {
                lit.0.iter().map(|&b| fold_byte(b)).collect()
            } else {
                lit.0.to_vec()
            };
            Info::exact(set([bytes])).simplify(false)
        }
        HirKind::Class(class) => match class_strings(class, ci) {
            Some(strings) => Info::exact(strings),
            None => Info::any_char(),
        },
        HirKind::Capture(cap) => analyze(&cap.sub, ci),
        HirKind::Repetition(rep) => {
            let sub = analyze(&rep.sub, ci);
            match (rep.min, rep.max) {
                (0, Some(1)) => alternate(sub, Info::exact(set([Vec::new()]))),
                (0, _) => Info::any(),
                // x+ and x{n,}: at least one copy, with anything after.
                (_, _) => {
                    let one = sub.clone().simplify(true);
                    Info {
                        can_empty: false,
                        exact: None,
                        prefix: one.prefix,
                        suffix: one.suffix,
                        query: one.query,
                    }
                }
            }
        }
        HirKind::Concat(items) => items
            .iter()
            .map(|h| analyze(h, ci))
            .reduce(concat)
            .unwrap_or_else(|| Info::exact(set([Vec::new()]))),
        HirKind::Alternation(branches) => branches
            .iter()
            .map(|h| analyze(h, ci))
            .reduce(alternate)
            .unwrap_or_else(Info::any),
    }
}

/// The trigram query a search for `pattern` must satisfy.
///
/// `case_insensitive` must match the index being queried: a folded index
/// stores folded trigrams, so literals are folded the same way. Under
/// case-insensitivity, a class that is a letter's Unicode fold set (`k`
/// with the Kelvin sign) is enumerated with every member folded to ASCII
/// where it is ASCII; a match only through a non-ASCII member keeps its own
/// bytes, so it is not pruned.
pub fn required_trigrams_for_regex(pattern: &str, case_insensitive: bool) -> Result<TrigramQuery> {
    let hir = ParserBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .utf8(false)
        .build()
        .parse(pattern)
        .map_err(|e| Error::InvalidPattern(e.to_string()))?;
    let info = analyze(&hir, case_insensitive).simplify(true);
    let affixes = match &info.exact {
        Some(e) => or_query(e),
        None => TrigramQuery::and(vec![or_query(&info.prefix), or_query(&info.suffix)]),
    };
    Ok(TrigramQuery::and(vec![info.query, affixes]))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::trigram::{pack, trigrams};

    fn tri(s: &str) -> TrigramQuery {
        let b = s.as_bytes();
        TrigramQuery::Trigram(pack(b[0], b[1], b[2]))
    }

    fn holds(q: &TrigramQuery, doc: &BTreeSet<u32>) -> bool {
        match q {
            TrigramQuery::All => true,
            TrigramQuery::Trigram(t) => doc.contains(t),
            TrigramQuery::And(qs) => qs.iter().all(|q| holds(q, doc)),
            TrigramQuery::Or(qs) => qs.iter().any(|q| holds(q, doc)),
        }
    }

    #[test]
    fn concatenation_crosses_alternatives() {
        let q = required_trigrams_for_regex("(abc|def)ghi", false).unwrap();
        let doc = |s: &str| trigrams(s.as_bytes(), false);
        assert!(holds(&q, &doc("xxabcghixx")));
        assert!(holds(&q, &doc("defghi")));
        assert!(!holds(&q, &doc("abc def ghi")), "{q:?}");
    }

    #[test]
    fn short_alternatives_join_their_neighbours() {
        // `a|b` alone constrains nothing, but `x(a|b)yz` requires xayz or xbyz.
        let q = required_trigrams_for_regex("x(a|b)yz", false).unwrap();
        assert!(holds(&q, &trigrams(b"xayz", false)));
        assert!(!holds(&q, &trigrams(b"xcyz", false)));
    }

    #[test]
    fn classes_gaps_and_optionals() {
        let q = required_trigrams_for_regex(r"fn\s+main", false).unwrap();
        assert!(holds(&q, &trigrams(b"fn  main", false)));
        assert!(!holds(&q, &trigrams(b"fn mane", false)));
        assert!(required_trigrams_for_regex("(abc)?", false)
            .unwrap()
            .is_all());
        assert!(required_trigrams_for_regex("ab", false).unwrap().is_all());
        assert_eq!(
            required_trigrams_for_regex("(abc)+", false).unwrap(),
            tri("abc")
        );
    }

    #[test]
    fn case_insensitive_folds() {
        let q = required_trigrams_for_regex("HeLLo", true).unwrap();
        assert!(holds(&q, &trigrams(b"say HELLO", true)));
        assert!(required_trigrams_for_regex("[a-z]bc", true)
            .unwrap()
            .is_all());
        assert!(matches!(
            required_trigrams_for_regex("(", false),
            Err(Error::InvalidPattern(_))
        ));
    }

    fn pattern() -> impl Strategy<Value = String> {
        let atom = prop_oneof![
            "[abcd]{1,4}".prop_map(|s| s),
            Just(".".to_string()),
            Just("[ab]".to_string()),
            Just(r"\s".to_string()),
        ];
        atom.prop_recursive(3, 12, 3, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 1..4).prop_map(|v| v.concat()),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("(?:{a}|{b})")),
                inner.clone().prop_map(|a| format!("(?:{a})?")),
                inner.clone().prop_map(|a| format!("(?:{a})+")),
                inner.prop_map(|a| format!("(?:{a})*")),
            ]
        })
    }

    proptest! {
        /// Soundness: whatever the regex matches satisfies the query.
        #[test]
        fn a_matching_document_always_satisfies_the_query(
            p in pattern(),
            text in "[abcd \n]{0,40}",
            ci in any::<bool>(),
        ) {
            let re = regex::bytes::RegexBuilder::new(&p).case_insensitive(ci).build().unwrap();
            let q = required_trigrams_for_regex(&p, ci).unwrap();
            if re.is_match(text.as_bytes()) {
                prop_assert!(holds(&q, &trigrams(text.as_bytes(), ci)), "{p:?} on {text:?}: {q:?}");
            }
        }
    }
}
