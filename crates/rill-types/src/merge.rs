//! The conflict algebra.
//!
//! A [`Merge<T>`] represents the result of merging N+1 "sides" against N
//! "bases": the value `A + (C - B) + (E - D)` where `A, C, E` are adds and
//! `B, D` are removes. A resolved value is a merge with one add and no
//! removes. This representation (taken from Jujutsu) has two properties that
//! matter for a system where merges must never fail:
//!
//! * it is closed under further merging and rebasing, so a conflict can be
//!   carried through any number of operations without nesting, and
//! * add/remove pairs that are equal cancel, so a conflict simplifies back to
//!   a plain value as soon as one side's change is undone or the base catches
//!   up.
//!
//! Terms are stored interleaved: `[add0, remove0, add1, remove1, ..., addN]`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Merge<T> {
    values: Vec<T>,
}

impl<T> Merge<T> {
    /// A resolved (conflict-free) value.
    pub fn resolved(value: T) -> Self {
        Merge {
            values: vec![value],
        }
    }

    /// Absent on every side; the conflict-free "deleted" value for
    /// `Merge<Option<T>>`.
    pub fn absent() -> Merge<Option<T>> {
        Merge::resolved(None)
    }

    /// Build from `N` removes and `N + 1` adds.
    pub fn from_removes_adds(
        removes: impl IntoIterator<Item = T>,
        adds: impl IntoIterator<Item = T>,
    ) -> Self {
        let mut adds = adds.into_iter();
        let mut values = Vec::new();
        values.push(adds.next().expect("a merge needs at least one add"));
        for (remove, add) in removes.into_iter().zip(adds.by_ref()) {
            values.push(remove);
            values.push(add);
        }
        assert!(
            adds.next().is_none(),
            "a merge needs exactly one more add than removes"
        );
        Merge { values }
    }

    /// Build from an interleaved term list; the length must be odd.
    pub fn from_vec(values: Vec<T>) -> Result<Self> {
        if values.len() % 2 == 0 {
            return Err(Error::InvalidMerge(format!(
                "term list must have odd length, got {}",
                values.len()
            )));
        }
        Ok(Merge { values })
    }

    /// Three-way merge term `left + (right - base)`.
    pub fn three_way(base: T, left: T, right: T) -> Self {
        Merge {
            values: vec![left, base, right],
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.values
    }

    /// Number of sides (adds).
    pub fn num_sides(&self) -> usize {
        self.values.len() / 2 + 1
    }

    pub fn is_resolved(&self) -> bool {
        self.values.len() == 1
    }

    pub fn as_resolved(&self) -> Option<&T> {
        if self.is_resolved() {
            Some(&self.values[0])
        } else {
            None
        }
    }

    pub fn into_resolved(self) -> std::result::Result<T, Self> {
        if self.is_resolved() {
            Ok(self.values.into_iter().next().unwrap())
        } else {
            Err(self)
        }
    }

    pub fn adds(&self) -> impl ExactSizeIterator<Item = &T> {
        self.values.iter().step_by(2)
    }

    pub fn removes(&self) -> impl ExactSizeIterator<Item = &T> {
        self.values.iter().skip(1).step_by(2)
    }

    /// Iterate all terms in interleaved order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        self.values.iter()
    }

    /// First add, which conventionally is "our" side.
    pub fn first(&self) -> &T {
        &self.values[0]
    }

    pub fn map<U>(&self, f: impl FnMut(&T) -> U) -> Merge<U> {
        Merge {
            values: self.values.iter().map(f).collect(),
        }
    }

    /// Pair each term with the corresponding term of `other`.
    pub fn zip<'a, U>(&'a self, other: &'a Merge<U>) -> Merge<(&'a T, &'a U)> {
        assert_eq!(
            self.values.len(),
            other.values.len(),
            "zipped merges must have the same shape"
        );
        Merge {
            values: self.values.iter().zip(other.values.iter()).collect(),
        }
    }
}

impl<T: PartialEq> Merge<T> {
    /// Cancel equal add/remove pairs. The result has the same "value" but
    /// fewer terms, and is resolved whenever a conflict has been undone.
    pub fn simplify(self) -> Self {
        // Work on the two lists separately: deleting a pair from the
        // interleaved vector would shift the parity of the terms between
        // them and change the meaning of the merge.
        let mut adds: Vec<T> = Vec::with_capacity(self.values.len() / 2 + 1);
        let mut removes: Vec<T> = Vec::with_capacity(self.values.len() / 2);
        for (i, v) in self.values.into_iter().enumerate() {
            if i % 2 == 0 {
                adds.push(v);
            } else {
                removes.push(v);
            }
        }
        let mut i = 0;
        while i < adds.len() {
            match removes.iter().position(|r| *r == adds[i]) {
                Some(j) => {
                    adds.remove(i);
                    removes.remove(j);
                    // A later add may now cancel against an earlier remove.
                    i = 0;
                }
                None => i += 1,
            }
        }
        Merge::from_removes_adds(removes, adds)
    }

    /// Resolve trivially: after simplification, a merge whose adds are all
    /// equal has that value regardless of its removes.
    pub fn resolve_trivial(&self) -> Option<&T>
    where
        T: Clone,
    {
        if let Some(v) = self.as_resolved() {
            return Some(v);
        }
        let simplified = self.clone().simplify();
        if simplified.is_resolved() {
            // The simplified value is owned; return the equal term from self.
            let v = &simplified.values[0];
            return self.values.iter().find(|x| *x == v);
        }
        let mut adds = simplified.adds();
        let first = adds.next()?;
        if adds.all(|a| a == first) {
            return self.values.iter().find(|x| *x == first);
        }
        None
    }

    /// Resolve trivially into an owned value.
    pub fn into_trivially_resolved(self) -> std::result::Result<T, Self>
    where
        T: Clone,
    {
        let simplified = self.simplify();
        if simplified.is_resolved() {
            return simplified.into_resolved();
        }
        let adds: Vec<&T> = simplified.adds().collect();
        if adds.iter().all(|a| *a == adds[0]) {
            return Ok(adds[0].clone());
        }
        Err(simplified)
    }
}

impl<T> Merge<Merge<T>> {
    /// Substitute nested merges into a single flat merge.
    ///
    /// For an inner merge at an "add" position its terms are copied as-is;
    /// at a "remove" position the inner adds and removes swap roles, because
    /// negating `(A - B)` gives `(B - A)`.
    pub fn flatten(self) -> Merge<T> {
        let mut values = Vec::new();
        // An inner list has odd length, so splicing it in keeps the outer
        // interleaving intact. At a remove slot the inner terms land on
        // alternating remove/add/remove positions, which negates the inner
        // value term by term: -(a0 - r0 + a1) == -a0 + r0 - a1.
        for inner in self.values {
            values.extend(inner.values);
        }
        Merge { values }
    }
}

impl<T> Merge<Option<T>> {
    /// Present on every side.
    pub fn is_present_everywhere(&self) -> bool {
        self.values.iter().all(Option::is_some)
    }
}

impl<T: fmt::Debug> fmt::Debug for Merge<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(v) = self.as_resolved() {
            return write!(f, "Resolved({v:?})");
        }
        write!(f, "Conflict(")?;
        for (i, v) in self.values.iter().enumerate() {
            if i > 0 {
                f.write_str(if i % 2 == 1 { " - " } else { " + " })?;
            }
            write!(f, "{v:?}")?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_way_shape() {
        let m = Merge::three_way(0, 1, 2);
        assert_eq!(m.num_sides(), 2);
        assert_eq!(m.adds().copied().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(m.removes().copied().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn simplify_cancels_pairs() {
        // left changed nothing: 0 + (2 - 0) -> 2
        let m = Merge::three_way(0, 0, 2).simplify();
        assert_eq!(m.as_resolved(), Some(&2));
        // both made the same change: 1 + (1 - 0) stays a term but resolves trivially
        let m = Merge::three_way(0, 1, 1);
        assert_eq!(m.resolve_trivial(), Some(&1));
        // real conflict
        let m = Merge::three_way(0, 1, 2);
        assert_eq!(m.resolve_trivial(), None);
    }

    #[test]
    fn rebase_simplifies_away_base() {
        // conflict C + (B - A) rebased onto D: D + ((C + (B - A)) - C)
        let conflict = Merge::three_way('A', 'C', 'B');
        let rebased = Merge::three_way(Merge::resolved('C'), Merge::resolved('D'), conflict)
            .flatten()
            .simplify();
        assert_eq!(rebased.as_slice(), &['D', 'A', 'B']);
    }

    #[test]
    fn revert_of_conflict_is_clean() {
        // E = C + (B - A); E + (C - E) == C
        let e = Merge::three_way('A', 'C', 'B');
        let reverted = Merge::three_way(e.clone(), e, Merge::resolved('C'))
            .flatten()
            .simplify();
        assert_eq!(reverted.as_resolved(), Some(&'C'));
    }

    #[test]
    fn from_vec_rejects_even() {
        assert!(Merge::from_vec(vec![1, 2]).is_err());
        assert!(Merge::from_vec(vec![1, 2, 3]).is_ok());
    }
}
