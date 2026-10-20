# Objects and ids

Everything Rill stores is either an immutable, content-addressed object or a
small mutable record that points at one. Immutable objects are named by the
BLAKE3 hash of their canonical encoding; mutable records are named by random
128-bit ids or a string key. All of these types live in `rill-types`, which
has no I/O and no async.

```mermaid
flowchart TD
    subgraph mutable["Mutable records (meta store)"]
        REF["Ref"]
        WS["Workspace"]
        RS["ReviewState (by ChangeId)"]
        IN["Intent"]
    end
    subgraph immutable["Immutable objects (object store)"]
        CV["ChangeVersion"]
        T["Tree"]
        B["Blob"]
        M["BlobManifest"]
        C["Chunk"]
    end
    REF & WS & RS --> CV
    CV -->|parents, predecessor| CV
    CV -->|intent| IN
    CV -. "root tree is derived" .-> T
    CV -->|FileChange::Set| B
    T -->|Dir| T
    T -->|File| B
    T -->|Chunked| M --> C
```

## Hashing

`ObjectHash` is 32 bytes, shown as hex (`short()` is the first 12 hex
characters). `ObjectHash::digest(kind, bytes)` is BLAKE3 over
`"rill:" ++ kind.as_str() ++ ":" ++ bytes`. The `ObjectKind` is a domain
separator, so equal bytes stored as different kinds never collide.

| `ObjectKind` | `as_str()` | What it names |
| --- | --- | --- |
| `Blob = 1` | `blob` | raw file bytes |
| `Chunk = 2` | `chunk` | raw bytes of one FastCDC chunk |
| `BlobManifest = 3` | `manifest` | encoded chunk list |
| `Tree = 4` | `tree` | encoded directory |
| `ChangeVersion = 5` | `cv` | encoded change version |
| `Derived = 6` | `derived` | derived objects such as `HistoryNode` |

`digest_parallel` (rayon tree hashing) and the streaming `hasher(kind)` give
the same digest as `digest`. `ObjectHash::ZERO` is a sentinel.

## Ids

`ChangeId` (`ch_`), `IntentId` (`in_`), `WorkspaceId` (`ws_`),
`PrincipalId` (`pr_`), `RepoId` (`rp_`) and `OperationId` (`op_`) are 16
bytes, displayed as `<prefix>_<hex>`; parsing accepts the hex with or
without the prefix. `new()` is random. `derived(namespace, name)` is the
first 16 bytes of BLAKE3 over `"rill:id:<prefix>:<namespace>:<name>"`;
`Principal::human/agent/service` use it so a principal's id follows from its
name. `OperationId` doubles as the idempotency key of write RPCs.

`TrunkSeq` is a `u64` shown as `trunk@N`. `RefName` is a validated string
of at most 256 characters from `[A-Za-z0-9/_.:@-]`, with no leading or
trailing `/`, no `//` and no `..`. `Timestamp` is `i64` milliseconds since
the Unix epoch (UTC), formatted as RFC 3339.

`RepoPathBuf` is a `/`-separated relative path, stored in Unicode NFC so
one name has one spelling. Leading and trailing `/` are trimmed and the
empty path is the root. `.`, `..`, empty components, `\\` and control
characters are rejected, as are components over 255 bytes and paths over
4096 bytes.

## Canonical encoding

An `Encodable` type declares `KIND`, `SCHEMA` (a `u16`) and `NAME`.
`encode` writes a 3-byte header, `[kind u8][schema u16 BE]`, then the
postcard body. `decode` rejects a different kind or schema with
`Error::UnsupportedSchema { kind, found, expected }`, so bytes are never
decoded as the wrong type. `hash()` is `digest(KIND, encode())`.
`ChangeVersion`, `Tree` and `BlobManifest` are `Encodable` at schema 1; so
is `HistoryNode` in `rill-core`, under kind `Derived`. Blobs and chunks are
not encoded: their bytes are the content.

Rill is pre-release, so every schema is 1 and a format change edits schema
1 in place; stores are recreated, not migrated. Golden fixtures in
`rill-types/tests/fixtures/` pin the bytes of each encoded type, so an
accidental format change fails a test. Regenerate them deliberately with
`RILL_UPDATE_FIXTURES=1`.

The encoding is deterministic because postcard has no field reordering,
every map in the model is a `BTreeMap`, and no encoded type uses
`skip_serializing_if` (one value with two encodings would have two hashes).
`#[serde(default)]` only affects decoding and is used on fields added after
version 1.

Mutable records are framed by `encode_record`, never hashed: see
[storage and keyspace](storage-and-keyspace.md).

## Tree

`Tree { entries: BTreeMap<String, TreeEntry> }`; an entry is
`File { content: ContentRef, kind: FileKind }` or `Dir { hash, summary }`.
`DirSummary` (`file_count`, `dir_count`, `total_size`) covers the whole
subtree, so size queries never walk. `FileKind` is `Regular`, `Executable`
or `Symlink`. `ContentRef` is in [content and chunking](content-and-chunking.md).

## ChangeVersion

A `ChangeVersion` is one concrete version of a change. Its hash is the
version identity; `change_id` is the stable identity across amends and
rebases. It stores only what changed relative to `parents[0]`.

| Field | Meaning |
| --- | --- |
| `change_id` | stable `ChangeId` |
| `parents` | empty for a root, one for a linear change, more for a merge |
| `predecessor` | the version this one amends or rebases |
| `subtree_changes` | per directory: `Copy { from_path, from_version }` (reuses the tree hash) or `Delete`; applied before file changes |
| `file_changes` | per path: a `FileChange` |
| `author`, `provenance` | `Principal`; `Provenance` (`tool`, `model`, `session`, `prompt_ref`, `sponsor`, `resolved_by`, `extra`) is hashed in |
| `intent`, `message`, `created_at`, `extra` | the why, free text, time, extension slots |

`FileChange` has no add/modify split:

* `Set { content, kind, copy_from }`: the path has this content. A rename is
  a `Set` with `copy_from: Some(CopySource { path, version })` plus a
  `Delete` of the source.
* `Delete`: the path is gone. It may be a prefix of `Set` paths (a file
  replaced by a directory).
* `Conflict(Merge<FileState>)`: unresolved; see
  [merge algebra](merge-algebra.md).

The root tree is not a field. It is [derived data](derived-data.md):
the first parent's tree plus the changes. For a conflicted path the first
add of the merge supplies the tree entry, so trees never contain conflicts.

`ChangeInfo` is the projection without `file_changes`, so history listings
never load a large change map. `Signature` (`signer`, Ed25519 `public_key`
and `signature`, `signed_at`) is stored beside a version, not in its hash.

The server owns authorship. `CreateChangeVersion` replaces `author` with
the authenticated caller and `created_at` with the server's clock, and
clears `provenance.sponsor` and `provenance.resolved_by`; the rest of
`provenance` is recorded as the client's claim. After storing a version the
server signs its hash with its attestation key (principal
`service:rill-server`) and appends that signature, so any stored version can
be traced to the server that accepted it.

## Change limits

`rill_types::limits::check_change` bounds every version the server stores,
and `rill_core::validate_version` adds the checks that need content:

| Rule | Limit |
| --- | --- |
| Paths changed (file plus subtree) | 100,000 |
| Encoded version size | 8 MiB |
| File size | 5 GiB |
| Terms in one conflict | 63 (32 sides) |
| `extra` fields | 32, keys up to 64 bytes, values up to 4 KiB |
| Symlink target | relative, at most 4096 bytes, never climbing above the root |
| Reserved paths | `.rill/checkout.toml`, `.rill/manifest.toml`: not set, copied into or linked to |

Symlink targets are checked whether inline or stored as a blob. Adding a
name that differs only in case from a sibling is rejected; collisions
already in the tree do not block unrelated edits. A repository created with
`windows_safe` also rejects names Windows cannot create (`CON`, `nul.txt`,
trailing dots or spaces, `:*?"<>|`).

## Mutable records

`Workspace` and `Operation`: [workspaces](../04-workflow/workspaces.md).
`ReviewState`, `Label`, `LabelVote`, `Thread`:
[review](../04-workflow/review.md). `Intent`, `RiskClass`:
[intents](../04-workflow/intents.md). `Principal`:
[identity and auth](../05-policy/identity-and-auth.md). `RefUpdate` is a
compare-and-swap request: `expected: None` means the ref must not exist,
`target: None` deletes it.

## Limitations

* Nothing in `rill-types` verifies a `Signature`; clients do not yet check
  the server's attestation.
* `ChangeInfo.files_changed` counts `file_changes` only.
* `Principal.identity` and `Provenance` fields are unvalidated strings.
* The `trunk` / `release/<name>` / `<owner>/<name>` ref taxonomy is a
  convention; `RefName` only checks characters.
* The types allow a change to reference content that was never uploaded;
  the server checks `Set` content on `PutChange`, not `Conflict` terms.

## Code

* [hash.rs](../../crates/rill-types/src/hash.rs), [ids.rs](../../crates/rill-types/src/ids.rs), [encoding.rs](../../crates/rill-types/src/encoding.rs)
* [path.rs](../../crates/rill-types/src/path.rs), [limits.rs](../../crates/rill-types/src/limits.rs), [rill-core validate.rs](../../crates/rill-core/src/validate.rs), [time.rs](../../crates/rill-types/src/time.rs), [principal.rs](../../crates/rill-types/src/principal.rs), [error.rs](../../crates/rill-types/src/error.rs)
* [objects/tree.rs](../../crates/rill-types/src/objects/tree.rs), [objects/change.rs](../../crates/rill-types/src/objects/change.rs), [objects/blob.rs](../../crates/rill-types/src/objects/blob.rs), [objects/refs.rs](../../crates/rill-types/src/objects/refs.rs)
* [objects/workspace.rs](../../crates/rill-types/src/objects/workspace.rs), [objects/review.rs](../../crates/rill-types/src/objects/review.rs), [objects/intent.rs](../../crates/rill-types/src/objects/intent.rs)

---
← Previous: [Glossary](../01-overview/glossary.md) · [Index](../index.md) · Next: [Storage and keyspace](storage-and-keyspace.md) →
