//! The contended landing profile (the phase 17 gate): agents append
//! entries to a few hot registry files, add dependencies that each rewrite
//! one lockfile, and edit files of their own. The repository declares the
//! registries `union` and the lockfile `regenerate` (an offline command),
//! so every request should land: one that comes back is a give-up.

use std::collections::BTreeMap;

use bytes::Bytes;
use rand::RngExt;
use rill_sdk::{Edit, RepoClient, Root};
use rill_types::{ChangeVersionHash, FileKind, RepoPathBuf};
use serde_json::{json, Value};

use crate::workload::Workload;

/// Hot registry files every agent appends to.
pub const REGISTRIES: usize = 5;
/// The lockfile every dependency change rewrites.
pub const LOCKFILE: &str = "deps.lock";

const MERGE_RULES: &str = r#"[[driver]]
paths = ["registry/*.txt"]
merge = "union"

[[driver]]
name = "deps"
paths = ["deps.lock"]
merge = "regenerate"
command = ["sh", "-c", "ls deps > deps.lock"]
inputs = ["deps/**", "deps.lock"]
timeout_secs = 60
"#;

fn set(path: &str, body: impl Into<Bytes>) -> Edit {
    Edit::Set {
        path: RepoPathBuf::new(path).expect("generated paths are valid"),
        bytes: body.into(),
        kind: FileKind::Regular,
    }
}

/// A plain list, which the structural merger leaves to the union driver.
fn registry(n: usize) -> String {
    format!("registry/r{n}.txt")
}

/// What the seed adds for the contended profile.
pub fn seed_edits() -> Vec<Edit> {
    let mut edits = vec![
        set(".rill/merge.toml", MERGE_RULES),
        set("deps/seed", "1\n"),
        set(LOCKFILE, "seed\n"),
    ];
    edits.extend((0..REGISTRIES).map(|n| set(&registry(n), format!("# registry {n}\n"))));
    edits
}

impl Workload {
    /// A contended change made at trunk's current head: the root it is
    /// based on and its edits. `None` when the head cannot be read.
    pub(crate) async fn contended_edits(&self, id: &str) -> Option<(Root, Vec<Edit>)> {
        let (_, head) = self.repo.trunk_head().await.ok().flatten()?;
        let at = Root::Version(head);
        let roll = rand::rng().random_range(0..100);
        let edits = match roll {
            0..50 => {
                let path = registry(rand::rng().random_range(0..REGISTRIES));
                let mut body = self.read_at(&at, &path).await?;
                body.extend_from_slice(format!("entry-{id}\n").as_bytes());
                vec![set(&path, body)]
            }
            50..80 => {
                let mut lock = self.read_at(&at, LOCKFILE).await?;
                lock.extend_from_slice(format!("{id}\n").as_bytes());
                vec![set(&format!("deps/{id}"), "1\n"), set(LOCKFILE, lock)]
            }
            _ => vec![set(
                &format!("src/own/{id}.rs"),
                format!("pub const ID: &str = \"{id}\";\n"),
            )],
        };
        Some((at, edits))
    }

    async fn read_at(&self, at: &Root, path: &str) -> Option<Vec<u8>> {
        let path = RepoPathBuf::new(path).ok()?;
        match self.repo.read_file(at, &path).await {
            Ok(bytes) => Some(bytes.to_vec()),
            Err(e) => {
                self.stats.error_with("read_for_edit", &e);
                None
            }
        }
    }
}

/// How the landed `versions` were merged, from what each records: by
/// merge driver (`merge_drivers`, per driver) and by the structural
/// merger (`structural_merge`).
pub async fn resolutions(repo: &RepoClient, versions: &[ChangeVersionHash]) -> Value {
    let mut drivers: BTreeMap<String, u64> = BTreeMap::new();
    let (mut structural, mut unread) = (0u64, 0u64);
    for hash in versions {
        let Ok(cv) = repo.change_version(*hash).await else {
            unread += 1;
            continue;
        };
        if cv.extra.contains_key("structural_merge") {
            structural += 1;
        }
        if let Some(text) = cv.extra.get("merge_drivers") {
            for part in String::from_utf8_lossy(text).split("; ") {
                let name = part.split(':').next().unwrap_or(part).to_string();
                *drivers.entry(name).or_default() += 1;
            }
        }
    }
    json!({
        "landings": versions.len(),
        "by_driver": drivers,
        "structural": structural,
        "unread": unread,
    })
}
