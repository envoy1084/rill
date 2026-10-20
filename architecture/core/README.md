# rill-core

`rill-core` holds the pure algorithms of Rill: diff, merge, tree construction
and traversal, ancestry queries, rebase and change-version construction. Every
function is a deterministic computation over objects read from a
`rill_store::ObjectStore` or `RepoStore`; the crate holds no service state and
no RPC dependency. The invariant shaping every module is that cost scales
with the size of the change, never with the size of the repository.

| Page | Topic | Source |
| --- | --- | --- |
| [diff](diff.md) | Line tokens, Histogram line diff, hunks, unified rendering, binary sniffing, hash-pruned tree diff | `diff/` |
| [merge-and-rebase](merge-and-rebase.md) | N-way text merge over `Merge<T>`, file-state merge, conflict markers and parser, change-level merge, rebase | `merge/`, `rebase.rs` |
| [trees](trees.md) | `TreeOps` cache, `TreeBuilder` trie build, applying a `ChangeVersion` to a tree, lookups and sparse walks | `tree/` |
| [graph](graph.md) | `CommitGraph`: cached parents and generations, `is_ancestor`, `merge_base`, `log` walks | `graph/` |
| [builder](builder.md) | `ChangeBuilder`: assembling and storing a `ChangeVersion` | `builder.rs` |

`derive/` (derived-data framework, `RootTree`, `Generation`, blame, path
history) and `blobs.rs` (chunked content) are documented separately. Shared
plumbing: [`error.rs`](../../crates/rill-core/src/error.rs) (the `Error`
enum and `Result<T>`), [`util/lru.rs`](../../crates/rill-core/src/util/lru.rs)
(bounded `LruCache`: stamped entries, oldest eighth evicted per overflow),
[`lib.rs`](../../crates/rill-core/src/lib.rs) (re-exports).
