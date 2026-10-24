//! Which files a root has, kept in layers: a full base map for one root,
//! and small delta maps for the roots after it (one per landing), so a
//! landing costs what it changed, not the size of the repository.

mod hasher;
mod layer;
mod view;

pub use layer::{PathChange, PathEntry, PathLayer, MAX_DELTA_DEPTH};
pub use view::PathView;
