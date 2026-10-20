# Data

How Rill represents, identifies and persists everything: the object model in
`rill-types`, the storage traits and backends in `rill-store`, and the
content and derived-data machinery in `rill-core`. The single rule that runs
through every page: immutable objects are named by the BLAKE3 hash of their
canonical encoding, mutable state is a small set of records and append-only
logs, and nothing is ever rewritten in place.

| Page | Covers |
| --- | --- |
| [objects.md](objects.md) | Ids, hashing and canonical encoding; `Blob`, `Tree`, `ChangeVersion`, `FileChange`; refs, workspaces, review, intents, principals, time. |
| [merge-algebra.md](merge-algebra.md) | `Merge<T>`: adds and removes, `simplify`, trivial resolution, `flatten`, and how a conflict is carried in `FileChange::Conflict`. |
| [storage.md](storage.md) | `ObjectStore` and `MetaStore`, verification on `put`, the `RILZ` codec, the `keys.rs` keyspace, `RepoStore` accessors, and the memory, local, S3 and FoundationDB backends. |
| [derived-data.md](derived-data.md) | The `Derivable` trait and `DeriveManager`; root tree, generation, change summary, path history and blame; caching and what triggers computation. |
| [blobs.md](blobs.md) | Inline, single-blob and FastCDC-chunked content; thresholds, write and read paths, size checks. |

Crate layering: `rill-types` (no I/O) ← `rill-store` ← `rill-core`. See
[docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md) for the principles these
pages implement.
