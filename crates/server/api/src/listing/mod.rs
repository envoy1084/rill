//! What every List shares: pages and filters.

pub mod filter;
pub mod page;

pub use filter::{FieldValue, Filter, Kind, Record};
pub use page::{Page, Position};
