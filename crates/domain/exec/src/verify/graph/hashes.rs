//! Reading a build tool's target hashes: a label to a value that changes
//! whenever anything the target depends on does.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{Error, Result};

/// Target hashes as label to hash.
pub type TargetHashes = BTreeMap<String, String>;

/// Parse `bazel-diff generate-hashes` output (`{"hashes": {label: hash},
/// "metadata": ...}`, or a bare object of label to hash), or `buck2
/// targets --json --show-target-hash` output (an array of targets). A
/// hash that is not a string is compared by its JSON text.
pub fn parse_target_hashes(bytes: &[u8]) -> Result<TargetHashes> {
    let mut value: Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("not JSON: {e}")))?;
    if let Some(inner) = value.get_mut("hashes").filter(|h| h.is_object()) {
        value = inner.take();
    }
    let mut hashes = TargetHashes::new();
    match value {
        Value::Object(map) => {
            for (label, hash) in map {
                let hash = match hash {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                hashes.insert(label, hash);
            }
        }
        Value::Array(targets) => {
            for target in targets {
                let field = |name: &str| target.get(name).and_then(Value::as_str);
                let (Some(package), Some(name), Some(hash)) = (
                    field("buck.package"),
                    field("name"),
                    field("buck.target_hash"),
                ) else {
                    return Err(invalid(format!(
                        "a target without package, name and hash: {target}"
                    )));
                };
                hashes.insert(format!("{package}:{name}"), hash.to_string());
            }
        }
        _ => return Err(invalid("expected an object or an array".into())),
    }
    Ok(hashes)
}

fn invalid(reason: String) -> Error {
    Error::Config {
        path: "build graph output".into(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bazel_diff_output_with_its_metadata() {
        // As bazel-diff 63 writes it: hash~directHash, module graph aside.
        let hashes = parse_target_hashes(
            br#"{"hashes": {"//:greet": "5e7c~ed23", "//:greet.cc": "ba7c~ba7c"},
                 "metadata": {"moduleGraphJson": "{}"}}"#,
        )
        .unwrap();
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes["//:greet"], "5e7c~ed23");
    }

    #[test]
    fn reads_bare_label_maps() {
        let hashes = parse_target_hashes(
            br#"{"//a:lib": "Rule#1f2e", "//b:bin": {"hash": "aa", "directHash": "bb"}}"#,
        )
        .unwrap();
        assert_eq!(hashes["//a:lib"], "Rule#1f2e");
        assert!(hashes["//b:bin"].contains("aa"));
    }

    #[test]
    fn reads_buck2_targets() {
        let hashes = parse_target_hashes(
            br#"[{"buck.package": "root//app", "name": "main", "buck.target_hash": "9c"}]"#,
        )
        .unwrap();
        assert_eq!(hashes["root//app:main"], "9c");
        assert!(parse_target_hashes(br#"[{"name": "x"}]"#).is_err());
        assert!(parse_target_hashes(b"3").is_err());
    }
}
