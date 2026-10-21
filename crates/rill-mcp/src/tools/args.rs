//! Typed access to a tool call's `arguments` object.

use std::str::FromStr;

use rill_sdk::Root;
use rill_types::{RepoPathBuf, TrunkSeq};
use serde_json::{Map, Value};

use crate::error::{ToolError, ToolResult};

/// The `arguments` object of a `tools/call`.
#[derive(Debug, Clone, Default)]
pub struct Args(Map<String, Value>);

impl Args {
    /// Accept an object or `null`/absent (treated as empty).
    pub fn new(value: Value) -> ToolResult<Self> {
        match value {
            Value::Null => Ok(Args(Map::new())),
            Value::Object(map) => Ok(Args(map)),
            other => Err(ToolError::invalid(format!(
                "arguments must be an object, got {}",
                kind_of(&other)
            ))),
        }
    }

    /// The raw value of a key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key).filter(|v| !v.is_null())
    }

    /// A required string.
    pub fn str(&self, key: &str) -> ToolResult<&str> {
        self.opt_str(key)
            .ok_or_else(|| ToolError::invalid(format!("`{key}` is required")))
    }

    /// An optional string.
    pub fn opt_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// An optional string, or an error when present but not a string.
    pub fn opt_string(&self, key: &str) -> ToolResult<Option<String>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(other) => Err(ToolError::invalid(format!(
                "`{key}` must be a string, got {}",
                kind_of(other)
            ))),
        }
    }

    /// An optional unsigned integer.
    pub fn opt_u64(&self, key: &str) -> ToolResult<Option<u64>> {
        match self.get(key) {
            None => Ok(None),
            Some(v) => v.as_u64().map(Some).ok_or_else(|| {
                ToolError::invalid(format!("`{key}` must be a non-negative integer"))
            }),
        }
    }

    /// An optional signed integer.
    pub fn opt_i64(&self, key: &str) -> ToolResult<Option<i64>> {
        match self.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_i64()
                .map(Some)
                .ok_or_else(|| ToolError::invalid(format!("`{key}` must be an integer"))),
        }
    }

    /// A required signed integer.
    pub fn i64(&self, key: &str) -> ToolResult<i64> {
        self.opt_i64(key)?
            .ok_or_else(|| ToolError::invalid(format!("`{key}` is required")))
    }

    /// An optional boolean, defaulting to `default`.
    pub fn bool_or(&self, key: &str, default: bool) -> ToolResult<bool> {
        match self.get(key) {
            None => Ok(default),
            Some(v) => v
                .as_bool()
                .ok_or_else(|| ToolError::invalid(format!("`{key}` must be a boolean"))),
        }
    }

    /// An optional unsigned integer with a default, clamped to `u32`.
    pub fn limit(&self, key: &str, default: u32) -> ToolResult<u32> {
        Ok(self
            .opt_u64(key)?
            .map(|v| v.min(u32::MAX as u64) as u32)
            .unwrap_or(default))
    }

    /// A required value parsed with `FromStr` (ids, hashes, ref names).
    pub fn parse<T: FromStr>(&self, key: &str) -> ToolResult<T>
    where
        T::Err: std::fmt::Display,
    {
        let text = self.str(key)?;
        text.parse()
            .map_err(|e| ToolError::invalid(format!("`{key}`: {e}")))
    }

    /// An optional value parsed with `FromStr`.
    pub fn opt_parse<T: FromStr>(&self, key: &str) -> ToolResult<Option<T>>
    where
        T::Err: std::fmt::Display,
    {
        match self.opt_string(key)? {
            None => Ok(None),
            Some(text) => text
                .parse()
                .map(Some)
                .map_err(|e| ToolError::invalid(format!("`{key}`: {e}"))),
        }
    }

    /// A root (`trunk`, `trunk@N`, a version hash, a ref), defaulting to
    /// trunk.
    pub fn root_or_trunk(&self, key: &str) -> ToolResult<Root> {
        Ok(self.opt_parse(key)?.unwrap_or_else(Root::trunk))
    }

    /// A repository path, defaulting to the root.
    pub fn path_or_root(&self, key: &str) -> ToolResult<RepoPathBuf> {
        match self.opt_str(key) {
            None => Ok(RepoPathBuf::root()),
            Some(p) => Ok(RepoPathBuf::new(p)?),
        }
    }

    /// A required repository path.
    pub fn path(&self, key: &str) -> ToolResult<RepoPathBuf> {
        Ok(RepoPathBuf::new(self.str(key)?)?)
    }

    /// A trunk sequence from an integer or `trunk@N` string.
    pub fn seq(&self, key: &str) -> ToolResult<TrunkSeq> {
        match self.get(key) {
            Some(Value::Number(n)) => n
                .as_u64()
                .map(TrunkSeq)
                .ok_or_else(|| ToolError::invalid(format!("`{key}` must be a sequence number"))),
            Some(Value::String(s)) => s
                .parse()
                .map_err(|e| ToolError::invalid(format!("`{key}`: {e}"))),
            Some(_) => Err(ToolError::invalid(format!(
                "`{key}` must be a sequence number"
            ))),
            None => Err(ToolError::invalid(format!("`{key}` is required"))),
        }
    }

    /// An optional array of strings.
    pub fn opt_str_list(&self, key: &str) -> ToolResult<Vec<String>> {
        match self.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| ToolError::invalid(format!("`{key}` must hold strings")))
                })
                .collect(),
            Some(_) => Err(ToolError::invalid(format!("`{key}` must be an array"))),
        }
    }

    /// An optional array of repository paths.
    pub fn opt_paths(&self, key: &str) -> ToolResult<Vec<RepoPathBuf>> {
        self.opt_str_list(key)?
            .iter()
            .map(|p| RepoPathBuf::new(p).map_err(Into::into))
            .collect()
    }

    /// A required array of objects.
    pub fn objects(&self, key: &str) -> ToolResult<Vec<Args>> {
        match self.get(key) {
            Some(Value::Array(items)) => items.iter().cloned().map(Args::new).collect(),
            Some(_) => Err(ToolError::invalid(format!(
                "`{key}` must be an array of objects"
            ))),
            None => Err(ToolError::invalid(format!("`{key}` is required"))),
        }
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_accessors() {
        let a = Args::new(json!({
            "s": "x", "n": 3, "b": true, "at": "trunk@4", "seq": 9,
            "paths": ["a", "b/c"], "nul": null
        }))
        .unwrap();
        assert_eq!(a.str("s").unwrap(), "x");
        assert!(a.str("missing").is_err());
        assert_eq!(a.opt_u64("n").unwrap(), Some(3));
        assert!(a.bool_or("b", false).unwrap());
        assert_eq!(a.limit("n", 1).unwrap(), 3);
        assert_eq!(a.limit("zzz", 7).unwrap(), 7);
        assert!(matches!(
            a.root_or_trunk("at").unwrap(),
            Root::Seq(TrunkSeq(4))
        ));
        assert_eq!(a.root_or_trunk("nul").unwrap(), Root::trunk());
        assert_eq!(a.seq("seq").unwrap(), TrunkSeq(9));
        assert_eq!(a.opt_paths("paths").unwrap().len(), 2);
        assert!(a.path_or_root("nul").unwrap().is_root());
    }

    #[test]
    fn rejects_non_objects() {
        assert!(Args::new(json!([1])).is_err());
        assert!(Args::new(Value::Null).is_ok());
    }
}
