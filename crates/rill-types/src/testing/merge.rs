use proptest::prelude::*;

use crate::Merge;

/// A merge of one to `max_sides` sides over `term`.
pub fn merge_of<T: std::fmt::Debug + Clone + 'static>(
    term: impl Strategy<Value = T> + 'static,
    max_sides: usize,
) -> impl Strategy<Value = Merge<T>> {
    let term = term.boxed();
    (1..=max_sides.max(1))
        .prop_flat_map(move |sides| prop::collection::vec(term.clone(), 2 * sides - 1))
        .prop_map(|terms| Merge::from_vec(terms).expect("odd length by construction"))
}
