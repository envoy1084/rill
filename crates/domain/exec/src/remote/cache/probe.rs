//! Hermeticity probes: a sample of the results that enter the cache are
//! run again, and a re-run with different outputs means the action reads
//! something besides its inputs (the clock, the network, randomness).
//! Its cached result is evicted and its label flagged non-hermetic.

use std::collections::BTreeMap;

use rill_reapi_proto::reapi::ActionResult;
use rill_types::ids::random_id;

/// Whether to probe this result, with probability `rate`.
pub(crate) fn sampled(rate: f64) -> bool {
    if rate <= 0.0 {
        return false;
    }
    let bytes = random_id();
    let draw = u64::from_le_bytes(bytes[..8].try_into().unwrap_or_default());
    (draw as f64 / u64::MAX as f64) < rate
}

/// What differs between two runs of one action: exit code, and the
/// digest of each output file and directory. Logs are not compared; they
/// often hold times and paths that do not matter.
pub fn differences(first: &ActionResult, second: &ActionResult) -> Vec<String> {
    let mut out = Vec::new();
    if first.exit_code != second.exit_code {
        out.push(format!(
            "exit code {} then {}",
            first.exit_code, second.exit_code
        ));
    }
    let outputs = |r: &ActionResult| -> BTreeMap<String, String> {
        r.output_files
            .iter()
            .map(|f| {
                (
                    f.path.clone(),
                    f.digest.as_ref().map_or(String::new(), |d| d.hash.clone()),
                )
            })
            .chain(r.output_directories.iter().map(|d| {
                (
                    format!("{}/", d.path),
                    d.tree_digest
                        .as_ref()
                        .map_or(String::new(), |t| t.hash.clone()),
                )
            }))
            .collect()
    };
    let (a, b) = (outputs(first), outputs(second));
    for (path, hash) in &a {
        match b.get(path) {
            None => out.push(format!("{path} missing on the re-run")),
            Some(other) if other != hash => out.push(format!("{path} differs")),
            Some(_) => {}
        }
    }
    for path in b.keys().filter(|p| !a.contains_key(*p)) {
        out.push(format!("{path} only on the re-run"));
    }
    out
}

#[cfg(test)]
mod tests {
    use rill_reapi_proto::reapi::{Digest, OutputFile};

    use super::*;

    fn result(exit_code: i32, files: &[(&str, &str)]) -> ActionResult {
        ActionResult {
            exit_code,
            output_files: files
                .iter()
                .map(|(path, hash)| OutputFile {
                    path: path.to_string(),
                    digest: Some(Digest {
                        hash: hash.to_string(),
                        size_bytes: 1,
                    }),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn identical_outputs_match_and_any_change_is_named() {
        let a = result(0, &[("out/a", "11"), ("out/b", "22")]);
        assert!(differences(&a, &a.clone()).is_empty());
        let b = result(0, &[("out/a", "11"), ("out/b", "33"), ("out/c", "44")]);
        assert_eq!(
            differences(&a, &b),
            ["out/b differs", "out/c only on the re-run"]
        );
        assert_eq!(differences(&a, &result(1, &[])).len(), 3);
    }

    #[test]
    fn sampling_follows_the_rate_at_the_extremes() {
        assert!(!(0..100).any(|_| sampled(0.0)));
        assert!((0..100).all(|_| sampled(1.0)));
    }
}
