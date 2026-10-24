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

    /// Evaluate against a posting lookup. `lookup` returns the documents
    /// containing a trigram, or `None` when the trigram is absent from the
    /// index.
    pub fn evaluate<F>(&self, lookup: &F) -> Candidates
    where
        F: Fn(u32) -> Option<PostingList>,
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
        let lookup = |t: u32| -> Option<PostingList> {
            if t == pack(b'a', b'b', b'c') {
                Some(docs(&[1, 2, 3]))
            } else if t == pack(b'b', b'c', b'd') {
                Some(docs(&[2, 3, 4]))
            } else {
                None
            }
        };
        let q = TrigramQuery::literal(b"abcd");
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(docs(&[2, 3])));

        let q = TrigramQuery::or(vec![
            TrigramQuery::literal(b"abc"),
            TrigramQuery::literal(b"bcd"),
        ]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(docs(&[1, 2, 3, 4])));

        let q = TrigramQuery::and(vec![TrigramQuery::literal(b"xyz")]);
        assert_eq!(q.evaluate(&lookup), Candidates::Docs(docs(&[])));

        assert!(TrigramQuery::literal(b"ab").is_all());
        assert!(TrigramQuery::or(vec![TrigramQuery::All, TrigramQuery::literal(b"abc")]).is_all());
        assert_eq!(TrigramQuery::All.evaluate(&lookup), Candidates::All);
    }
}
