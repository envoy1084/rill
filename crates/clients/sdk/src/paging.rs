//! Reading every page of a list.

use std::future::Future;

use crate::error::Result;

/// Every item of a list, calling `page` with each page token (empty for
/// the first page) until it returns an empty next token.
pub(crate) async fn all<T, F, Fut>(mut page: F) -> Result<Vec<T>>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, String)>>,
{
    let mut out = Vec::new();
    let mut token = String::new();
    loop {
        let (items, next) = page(token).await?;
        out.extend(items);
        if next.is_empty() {
            return Ok(out);
        }
        token = next;
    }
}

/// The page size every collecting list asks for.
pub(crate) const PAGE: i32 = 1000;
