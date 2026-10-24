//! The boolean query a regex imposes on trigram postings, and its
//! evaluation into a candidate document set.

use super::{DocId, PostingList};

/// What a regex requires of a document's trigram set.
///
/// `All` means the regex constrains nothing (no literal of three or more
/// bytes is mandatory), so every document is a candidate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrigramQuery {
    /// Every document matches.
    All,
    /// The document must contain this trigram.
    Trigram(u32),
    /// Every sub-query must hold.
    And(Vec<TrigramQuery>),
    /// At least one sub-query must hold.
    Or(Vec<TrigramQuery>),
}

/// Where a query's posting lists come from.
pub trait Postings {
    /// A relative size of a trigram's posting list (its encoded bytes);
    /// `None` when the trigram is absent.
    fn size(&self, trigram: u32) -> Option<usize>;
    /// The documents containing a trigram; `None` when it is absent.
    fn get(&self, trigram: u32) -> Option<PostingList>;
}

impl<F: Fn(u32) -> Option<PostingList>> Postings for F {
    fn size(&self, trigram: u32) -> Option<usize> {
        self(trigram).map(|p| p.len() as usize)
    }

    fn get(&self, trigram: u32) -> Option<PostingList> {
        self(trigram)
    }
}

/// A conjunction stops intersecting once this few candidates remain:
/// verifying them costs less than reading more posting lists, and a
/// superset of the matches is still sound.
const NARROW_ENOUGH: u64 = 64;

/// The documents a [`TrigramQuery`] admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidates {
    /// No pruning possible: scan every document.
    All,
    /// Exactly these documents.
    Docs(PostingList),
}

impl Candidates {
    /// Whether `doc` is a candidate.
    pub fn contains(&self, doc: DocId) -> bool {
        match self {
            Candidates::All => true,
            Candidates::Docs(docs) => docs.contains(doc),
        }
    }

    fn narrow_enough(&self) -> bool {
        matches!(self, Candidates::Docs(d) if d.len() <= NARROW_ENOUGH)
    }

    fn and(self, other: Candidates) -> Candidates {
        match (self, other) {
            (Candidates::All, x) | (x, Candidates::All) => x,
            (Candidates::Docs(a), Candidates::Docs(b)) => Candidates::Docs(a & b),
        }
    }

    fn or(self, other: Candidates) -> Candidates {
        match (self, other) {
            (Candidates::All, _) | (_, Candidates::All) => Candidates::All,
            (Candidates::Docs(a), Candidates::Docs(b)) => Candidates::Docs(a | b),
        }
    }
}

impl TrigramQuery {
    /// Conjunction of the trigrams of a literal, case-folded as the index
    /// is; `All` if it is too short to contain one.
    pub fn literal(bytes: &[u8]) -> TrigramQuery {
        if bytes.len() < 3 {
            return TrigramQuery::All;
        }
        TrigramQuery::and(
            bytes
                .windows(3)
                .map(|w| {
                    let f = super::extract::fold_byte;
                    TrigramQuery::Trigram(super::pack(f(w[0]), f(w[1]), f(w[2])))
                })
                .collect(),
        )
    }

    /// Conjunction with the obvious simplifications.
    pub fn and(mut terms: Vec<TrigramQuery>) -> TrigramQuery {
        terms.retain(|t| *t != TrigramQuery::All);
        terms.sort();
        terms.dedup();
        match terms.len() {
            0 => TrigramQuery::All,
            1 => terms.pop().expect("one element"),
            _ => TrigramQuery::And(terms),
        }
    }

    /// Disjunction with the obvious simplifications: any unconstrained
    /// branch makes the whole disjunction unconstrained.
    pub fn or(terms: Vec<TrigramQuery>) -> TrigramQuery {
        if terms.is_empty() || terms.contains(&TrigramQuery::All) {
            return TrigramQuery::All;
        }
        let mut terms = terms;
        if terms.len() == 1 {
            return terms.pop().expect("one element");
        }
        TrigramQuery::Or(terms)
    }

    /// True if the query cannot prune anything.
    pub fn is_all(&self) -> bool {
        *self == TrigramQuery::All
    }

    /// Evaluate against posting lists: a superset of the documents the
    /// query admits. A conjunction intersects its trigrams smallest list
    /// first and stops once few candidates remain.
    pub fn evaluate<P: Postings + ?Sized>(&self, postings: &P) -> Candidates {
        match self {
            TrigramQuery::All => Candidates::All,
            TrigramQuery::Trigram(t) => Candidates::Docs(postings.get(*t).unwrap_or_default()),
            TrigramQuery::And(terms) => {
                let mut acc = Candidates::All;
                let mut sized = Vec::new();
                for term in terms {
                    match term {
                        TrigramQuery::Trigram(t) => match postings.size(*t) {
                            Some(n) => sized.push((n, *t)),
                            None => return Candidates::Docs(PostingList::new()),
                        },
                        other => acc = acc.and(other.evaluate(postings)),
                    }
                }
                sized.sort_unstable();
                for (_, t) in sized {
                    if acc.narrow_enough() {
                        break;
                    }
                    acc = acc.and(Candidates::Docs(postings.get(t).unwrap_or_default()));
                }
                acc
            }
            TrigramQuery::Or(terms) => terms
                .iter()
                .map(|t| t.evaluate(postings))
                .fold(Candidates::Docs(PostingList::new()), Candidates::or),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigram::pack;

    #[test]
    fn evaluates_boolean_structure() {
        let docs = |d: &[DocId]| d.iter().copied().collect::<PostingList>();
        // Large lists, so the conjunction intersects fully.
        let abc: PostingList = (0..1_000).collect();
        let bcd: PostingList = (500..2_000).collect();
        let lookup = |t: u32| -> Option<PostingList> {
            if t == pack(b'a', b'b', b'c') {
                Some(abc.clone())
            } else if t == pack(b'b', b'c', b'd') {
                Some(bcd.clone())
            } else {
                None
            }
        };
        let q = TrigramQuery::literal(b"abcd");
        assert_eq!(
            q.evaluate(&lookup),
            Candidates::Docs((500..1_000).collect())
        );

        let q = TrigramQuery::or(vec![
            TrigramQuery::literal(b"abc"),
            TrigramQuery::literal(b"bcd"),
        ]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs((0..2_000).collect()));

        let q = TrigramQuery::and(vec![TrigramQuery::literal(b"xyz")]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(docs(&[])));

        assert!(TrigramQuery::literal(b"ab").is_all());
        assert!(TrigramQuery::or(vec![TrigramQuery::All, TrigramQuery::literal(b"abc")]).is_all());
        assert_eq!(TrigramQuery::All.evaluate(&lookup), Candidates::All);
    }

    #[test]
    fn a_narrow_conjunction_stops_early_but_keeps_every_match() {
        let small: PostingList = [3, 7, 9].into_iter().collect();
        let big: PostingList = (0..100_000).filter(|d| d % 2 == 1).collect();
        let lookup = |t: u32| -> Option<PostingList> {
            match t {
                1 => Some(small.clone()),
                2 => Some(big.clone()),
                _ => None,
            }
        };
        let q = TrigramQuery::And(vec![TrigramQuery::Trigram(2), TrigramQuery::Trigram(1)]);
        let Candidates::Docs(got) = q.evaluate(&lookup) else {
            panic!("a conjunction of trigrams narrows");
        };
        assert!((&small & &big).is_subset(&got), "no match is lost");
        assert!(got.is_subset(&small), "the smallest list was read first");
        let missing = TrigramQuery::And(vec![TrigramQuery::Trigram(2), TrigramQuery::Trigram(9)]);
        assert_eq!(
            missing.evaluate(&lookup),
            Candidates::Docs(PostingList::new())
        );
    }
}
