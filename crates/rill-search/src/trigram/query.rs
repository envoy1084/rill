//! The boolean query a regex imposes on trigram postings, and its
//! evaluation into a candidate document set.

use super::postings::DocId;

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

/// The documents a [`TrigramQuery`] admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidates {
    /// No pruning possible: scan every document.
    All,
    /// Exactly these documents, sorted ascending.
    Docs(Vec<DocId>),
}

impl Candidates {
    /// Whether `doc` is a candidate.
    pub fn contains(&self, doc: DocId) -> bool {
        match self {
            Candidates::All => true,
            Candidates::Docs(docs) => docs.binary_search(&doc).is_ok(),
        }
    }

    fn and(self, other: Candidates) -> Candidates {
        match (self, other) {
            (Candidates::All, x) | (x, Candidates::All) => x,
            (Candidates::Docs(a), Candidates::Docs(b)) => Candidates::Docs(intersect(&a, &b)),
        }
    }

    fn or(self, other: Candidates) -> Candidates {
        match (self, other) {
            (Candidates::All, _) | (_, Candidates::All) => Candidates::All,
            (Candidates::Docs(a), Candidates::Docs(b)) => Candidates::Docs(union(&a, &b)),
        }
    }
}

impl TrigramQuery {
    /// Conjunction of the trigrams of a literal; `All` if it is too short to
    /// contain one.
    pub fn literal(bytes: &[u8]) -> TrigramQuery {
        if bytes.len() < 3 {
            return TrigramQuery::All;
        }
        TrigramQuery::and(
            bytes
                .windows(3)
                .map(|w| TrigramQuery::Trigram(super::pack(w[0], w[1], w[2])))
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

    /// Evaluate against a posting lookup. `lookup` returns the sorted
    /// documents containing a trigram, or `None` when the trigram is absent
    /// from the index.
    pub fn evaluate<F>(&self, lookup: &F) -> Candidates
    where
        F: Fn(u32) -> Option<Vec<DocId>>,
    {
        match self {
            TrigramQuery::All => Candidates::All,
            TrigramQuery::Trigram(t) => Candidates::Docs(lookup(*t).unwrap_or_default()),
            TrigramQuery::And(terms) => terms
                .iter()
                .map(|t| t.evaluate(lookup))
                .fold(Candidates::All, Candidates::and),
            TrigramQuery::Or(terms) => terms
                .iter()
                .map(|t| t.evaluate(lookup))
                .fold(Candidates::Docs(Vec::new()), Candidates::or),
        }
    }
}

fn intersect(a: &[DocId], b: &[DocId]) -> Vec<DocId> {
    let mut out = Vec::with_capacity(a.len().min(b.len()));
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn union(a: &[DocId], b: &[DocId]) -> Vec<DocId> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => {
                out.push(a[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(b[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigram::pack;

    #[test]
    fn evaluates_boolean_structure() {
        let lookup = |t: u32| -> Option<Vec<DocId>> {
            if t == pack(b'a', b'b', b'c') {
                Some(vec![1, 2, 3])
            } else if t == pack(b'b', b'c', b'd') {
                Some(vec![2, 3, 4])
            } else {
                None
            }
        };
        let q = TrigramQuery::literal(b"abcd");
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(vec![2, 3]));

        let q = TrigramQuery::or(vec![
            TrigramQuery::literal(b"abc"),
            TrigramQuery::literal(b"bcd"),
        ]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(vec![1, 2, 3, 4]));

        let q = TrigramQuery::and(vec![TrigramQuery::literal(b"xyz")]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(vec![]));

        assert!(TrigramQuery::literal(b"ab").is_all());
        assert!(TrigramQuery::or(vec![TrigramQuery::All, TrigramQuery::literal(b"abc")]).is_all());
        assert_eq!(TrigramQuery::All.evaluate(&lookup), Candidates::All);
    }
}
