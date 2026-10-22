//! Pages of a list (AIP-158): a size and an opaque token naming where the
//! next page starts, bound to the request's parameters.

use base64::Engine;

use rill_runtime::error::{Error, Result};

/// Items a page holds when the caller does not say.
pub const DEFAULT_PAGE_SIZE: usize = 100;
/// The most a page holds; larger requests are cut to it.
pub const MAX_PAGE_SIZE: usize = 1000;

const VERSION: u8 = 1;

/// Where a page starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    /// The first page.
    Start,
    /// After this many items of a stable ordering.
    Offset(u64),
    /// After the item with this key, for lists read by key.
    After(Vec<u8>),
}

/// One page asked for: how many items, from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub size: usize,
    pub start: Position,
    scope: [u8; 8],
}

impl Page {
    /// The page `page_size` and `page_token` ask for. `scope` lists what the
    /// token is bound to (parent, filter, order); a token issued for other
    /// values is refused.
    pub fn new(page_size: i32, page_token: &str, scope: &[&str]) -> Result<Self> {
        if page_size < 0 {
            return Err(Error::invalid("page_size must not be negative"));
        }
        let size = match page_size as usize {
            0 => DEFAULT_PAGE_SIZE,
            n => n.min(MAX_PAGE_SIZE),
        };
        let scope = digest(scope);
        let start = if page_token.is_empty() {
            Position::Start
        } else {
            decode(page_token, scope)?
        };
        Ok(Page { size, start, scope })
    }

    /// The token for the page after this one, starting at `next`.
    pub fn token(&self, next: Position) -> String {
        encode(self.scope, &next)
    }

    /// This page of `items`, which hold the whole list in a stable order,
    /// and the next page's token (empty after the last).
    pub fn of<T>(&self, items: Vec<T>) -> Result<(Vec<T>, String)> {
        let skip = match &self.start {
            Position::Start => 0,
            Position::Offset(n) => *n as usize,
            Position::After(_) => {
                return Err(Error::InvalidPageToken("not a token for this list".into()))
            }
        };
        let total = items.len();
        let page: Vec<T> = items.into_iter().skip(skip).take(self.size).collect();
        let end = skip + page.len();
        let next = if end < total {
            self.token(Position::Offset(end as u64))
        } else {
            String::new()
        };
        Ok((page, next))
    }
}

fn digest(scope: &[&str]) -> [u8; 8] {
    let mut hasher = blake3::Hasher::new();
    for part in scope {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let mut out = [0; 8];
    out.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    out
}

fn encode(scope: [u8; 8], position: &Position) -> String {
    let mut raw = vec![VERSION];
    raw.extend_from_slice(&scope);
    match position {
        Position::Start => raw.push(0),
        Position::Offset(n) => {
            raw.push(1);
            raw.extend_from_slice(&n.to_be_bytes());
        }
        Position::After(key) => {
            raw.push(2);
            raw.extend_from_slice(key);
        }
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

fn decode(token: &str, scope: [u8; 8]) -> Result<Position> {
    let bad = |why: &str| Error::InvalidPageToken(why.into());
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| bad("not a page token"))?;
    if raw.len() < 10 || raw[0] != VERSION {
        return Err(bad("not a page token"));
    }
    if raw[1..9] != scope {
        return Err(bad("issued for a different parent, filter or order"));
    }
    match raw[9] {
        0 => Ok(Position::Start),
        1 => {
            let n: [u8; 8] = raw[10..].try_into().map_err(|_| bad("not a page token"))?;
            Ok(Position::Offset(u64::from_be_bytes(n)))
        }
        2 => Ok(Position::After(raw[10..].to_vec())),
        _ => Err(bad("not a page token")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_walk_a_list_to_the_end_without_repeats_or_gaps() {
        let items: Vec<u32> = (0..250).collect();
        let mut seen = Vec::new();
        let mut token = String::new();
        loop {
            let page = Page::new(100, &token, &["orgs/o/repos/r", ""]).unwrap();
            let (items, next) = page.of(items.clone()).unwrap();
            seen.extend(items);
            if next.is_empty() {
                break;
            }
            token = next;
        }
        assert_eq!(seen, (0..250).collect::<Vec<_>>());
    }

    #[test]
    fn a_token_from_other_parameters_or_garbage_is_refused() {
        let first = Page::new(10, "", &["parent", "status = \"open\""]).unwrap();
        let (_, next) = first.of((0..50).collect::<Vec<u8>>()).unwrap();
        assert!(Page::new(10, &next, &["parent", "status = \"open\""]).is_ok());
        for scope in [["parent", ""], ["other", "status = \"open\""]] {
            assert!(matches!(
                Page::new(10, &next, &scope),
                Err(Error::InvalidPageToken(_))
            ));
        }
        assert!(matches!(
            Page::new(10, "!!", &["parent"]),
            Err(Error::InvalidPageToken(_))
        ));
    }

    #[test]
    fn sizes_default_and_cap() {
        assert_eq!(Page::new(0, "", &[]).unwrap().size, DEFAULT_PAGE_SIZE);
        assert_eq!(Page::new(5000, "", &[]).unwrap().size, MAX_PAGE_SIZE);
        assert!(Page::new(-1, "", &[]).is_err());
    }
}
