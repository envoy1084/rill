//! Running regex searches over an indexed root.

mod hit;
mod lines;
mod request;
mod search;

pub use hit::SearchHit;
pub use lines::LineIndex;
pub use request::{SearchQuery, DEFAULT_MAX_RESULTS};
pub use search::{search, PathFilter};
