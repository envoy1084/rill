//! Paginated list results.

/// One page of a paginated listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// Token for the next page; `None` on the last page.
    pub next_page_token: Option<String>,
}

impl<T> Page<T> {
    /// Build a page.
    pub fn new(items: Vec<T>, next_page_token: Option<String>) -> Self {
        Page {
            items,
            next_page_token,
        }
    }

    /// True when another page follows.
    pub fn has_more(&self) -> bool {
        self.next_page_token.is_some()
    }
}
