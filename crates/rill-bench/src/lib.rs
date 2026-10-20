//! Deterministic repositories for benchmarks: the same size and seed always
//! produce the same files, trees and history, so numbers are comparable
//! across machines and commits.

use std::sync::Arc;

use bytes::Bytes;
use rill_core::{ChangeBuilder, CommitGraph, DeriveManager, TreeOps};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeId, ChangeVersionHash, Principal, Timestamp};

/// Shape of a generated repository.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub files: usize,
    /// Directory fan-out at every level.
    pub fanout: usize,
    /// Lines per file.
    pub lines: usize,
    pub seed: u64,
}

impl Shape {
    /// A repository of `files` files: fan-out 32, 16 lines each.
    pub fn with_files(files: usize) -> Self {
        Shape {
            files,
            fanout: 32,
            lines: 16,
            seed: 7,
        }
    }

    /// File count from `RILL_BENCH_FILES`, else `default`.
    pub fn from_env(default: usize) -> Self {
        let files = std::env::var("RILL_BENCH_FILES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default);
        Self::with_files(files)
    }

    /// Path of file `i`: directories by fan-out, e.g. `d3/d17/f9.rs`.
    pub fn path(&self, i: usize) -> String {
        let mut parts = Vec::new();
        let mut rest = i / self.fanout;
        while rest > 0 {
            parts.push(format!("d{}", rest % self.fanout));
            rest /= self.fanout;
        }
        parts.reverse();
        parts.push(format!("f{}.rs", i % self.fanout));
        parts.join("/")
    }

    /// Content of file `i` at `revision`.
    pub fn content(&self, i: usize, revision: usize) -> Bytes {
        let mut out = String::new();
        for line in 0..self.lines {
            let tag = mix(self.seed ^ (i as u64) << 20 ^ line as u64) % 1000;
            if line == (i + revision) % self.lines && revision > 0 {
                out.push_str(&format!("let edited_{revision} = {tag};\n"));
            } else {
                out.push_str(&format!("let v{line} = {tag};\n"));
            }
        }
        Bytes::from(out)
    }
}

/// A generated repository with one root version holding every file.
pub struct Corpus {
    pub shape: Shape,
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub graph: Arc<CommitGraph>,
    pub root: ChangeVersionHash,
}

impl Corpus {
    /// Build the repository; cost is linear in the file count.
    pub async fn build(shape: Shape) -> Self {
        let stores = Stores::memory();
        let config = stores.create_repo("bench", "").await.expect("memory store");
        let repo = stores.repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), 1 << 16);
        let derive = DeriveManager::new(repo.clone(), trees, 1 << 14);
        let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));
        let mut builder = ChangeBuilder::new(author())
            .change_id(ChangeId::derived("bench", "root"))
            .created_at(Timestamp::from_millis(0));
        for i in 0..shape.files {
            builder = builder
                .file(&shape.path(i), shape.content(i, 0))
                .expect("generated paths are valid");
        }
        let (root, _) = builder.commit(&repo).await.expect("memory store");
        Corpus {
            shape,
            repo,
            derive,
            graph,
            root,
        }
    }

    /// A change on `parent` editing `count` files spread across the tree,
    /// as `revision`.
    pub async fn change(
        &self,
        parent: ChangeVersionHash,
        count: usize,
        revision: usize,
    ) -> ChangeVersionHash {
        let mut builder = ChangeBuilder::new(author())
            .change_id(ChangeId::derived("bench", &format!("change-{revision}")))
            .parent(parent)
            .created_at(Timestamp::from_millis(revision as i64));
        for k in 0..count {
            let i = self.spread(k, count, revision);
            builder = builder
                .file(&self.shape.path(i), self.shape.content(i, revision))
                .expect("generated paths are valid");
        }
        builder.commit(&self.repo).await.expect("memory store").0
    }

    /// The `k`th of `count` file indexes for `revision`, spread over the
    /// repository so changes touch many directories.
    pub fn spread(&self, k: usize, count: usize, revision: usize) -> usize {
        let stride = (self.shape.files / count.max(1)).max(1);
        (k * stride + revision * 7) % self.shape.files
    }
}

fn author() -> Principal {
    Principal::human("bench")
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
