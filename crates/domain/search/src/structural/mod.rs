//! Structural search: ast-grep patterns (`foo($A, $$$REST)`) matched
//! against syntax trees, narrowed by the trigram index first.

mod grammar;
mod pattern;

pub use grammar::{structural_languages, Grammar};
pub use pattern::StructuralPattern;
