//! Where objects and metadata live.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Storage backend selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Everything in process memory; lost on exit. For tests and demos.
    Memory,
    /// Objects as files and metadata in redb under `dir`.
    Local {
        /// Data directory; created if missing.
        dir: PathBuf,
    },
    /// Objects in an S3-compatible bucket, metadata in redb under
    /// `meta_dir` unless `[metadata]` says otherwise.
    S3 {
        /// Bucket URL understood by `object_store`, e.g. `s3://bucket`.
        url: String,
        /// Key prefix inside the bucket.
        #[serde(default)]
        prefix: String,
        /// Directory holding the redb metadata store; required unless
        /// metadata is in FoundationDB.
        #[serde(default)]
        meta_dir: Option<PathBuf>,
        /// Start even if the service lacks conditional put, checking for an
        /// object before writing it instead.
        #[serde(default)]
        allow_unconditional_put: bool,
    },
}

/// Where metadata lives, when not where `[storage]` puts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetadataConfig {
    /// Beside the objects: in memory for memory storage, in redb for local
    /// and S3 storage.
    #[default]
    Storage,
    /// FoundationDB (servers built with the `fdb` feature).
    Fdb {
        /// Cluster file; the platform default when absent.
        #[serde(default)]
        cluster_file: Option<PathBuf>,
        /// Name of this deployment's root subspace, so one cluster can
        /// hold several deployments.
        #[serde(default = "default_root")]
        root: String,
    },
}

fn default_root() -> String {
    "rill".into()
}
