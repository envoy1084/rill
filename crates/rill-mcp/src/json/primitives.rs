//! Scalars shared by every view: hashes, principals, timestamps, enums,
//! byte content and pages.

use rill_sdk::Page;
use rill_types::{ObjectHash, Principal, Timestamp};
use serde_json::{json, Value};

/// A hash as lowercase hex.
pub fn hash(h: &ObjectHash) -> Value {
    Value::String(h.to_hex())
}

/// `Some` through `f`, `None` as `null`.
pub fn opt<T>(value: Option<&T>, f: impl FnOnce(&T) -> Value) -> Value {
    value.map(f).unwrap_or(Value::Null)
}

/// RFC 3339 text plus epoch milliseconds.
pub fn timestamp(t: &Timestamp) -> Value {
    json!({ "rfc3339": t.to_rfc3339(), "millis": t.millis() })
}

/// A principal with its id, kind, name and display form.
pub fn principal(p: &Principal) -> Value {
    json!({
        "id": p.id.to_string(),
        "kind": p.kind.as_str(),
        "name": p.name,
        "identity": p.identity,
        "display": p.to_string(),
    })
}

/// The `snake_case` name of a fieldless enum variant, from its `Debug`
/// rendering (`InProgress` becomes `in_progress`).
pub fn enum_name<T: std::fmt::Debug>(value: &T) -> Value {
    let debug = format!("{value:?}");
    let mut out = String::with_capacity(debug.len() + 4);
    for (i, c) in debug.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    Value::String(out)
}

/// File content: `{"encoding":"utf8","text":...}` when valid UTF-8,
/// `{"encoding":"hex","hex":...}` otherwise, always with the byte size.
pub fn bytes(data: &[u8]) -> Value {
    match std::str::from_utf8(data) {
        Ok(text) => json!({ "encoding": "utf8", "size": data.len(), "text": text }),
        Err(_) => json!({ "encoding": "hex", "size": data.len(), "hex": hex::encode(data) }),
    }
}

/// A page of items with its continuation token.
pub fn page<T>(page: &Page<T>, item: impl Fn(&T) -> Value) -> Value {
    json!({
        "items": page.items.iter().map(item).collect::<Vec<_>>(),
        "next_page_token": page.next_page_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Sample {
        InProgress,
        Open,
    }

    #[test]
    fn enum_names_are_snake_case() {
        assert_eq!(enum_name(&Sample::InProgress), "in_progress");
        assert_eq!(enum_name(&Sample::Open), "open");
    }

    #[test]
    fn bytes_pick_encoding() {
        assert_eq!(bytes(b"hi")["encoding"], "utf8");
        assert_eq!(bytes(&[0xff, 0xfe])["encoding"], "hex");
        assert_eq!(bytes(&[0xff, 0xfe])["hex"], "fffe");
    }
}
