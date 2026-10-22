# Content and chunking

File content enters the object store through `rill_core::blobs::store_content`
and leaves it through `read_content`. The byte length alone picks the form:
inline in the change or tree entry, one `Blob` object, or a `BlobManifest`
over FastCDC `Chunk` objects. The thresholds are constants, so the same bytes
always get the same `ContentRef` and deduplicate across clients.

## Forms of content

```mermaid
flowchart TD
    IN["store_content(bytes)"] --> S1{"len <= 256 B"}
    S1 -- yes --> INL["ContentRef::Inline { bytes }<br/>nothing written"]
    S1 -- no --> S2{"len < 1 MiB"}
    S2 -- yes --> BLOB["put_blob: ObjectKind::Blob<br/>ContentRef::Blob { hash, size }"]
    S2 -- no --> CDC["fastcdc crate (v2020) on spawn_blocking<br/>16 / 64 / 256 KiB"]
    CDC --> CH["put each ObjectKind::Chunk"]
    CH --> MAN["put_object BlobManifest<br/>ContentRef::Chunked { hash, size }"]
```

| Constant | Value | Defined in |
| --- | --- | --- |
| `INLINE_THRESHOLD` | 256 bytes (inline at or below) | `rill-types` `objects/blob.rs` |
| `CHUNK_THRESHOLD` | 1 MiB (chunked at or above) | `rill-types` `objects/blob.rs` |
| `CHUNK_MIN` / `CHUNK_AVG` / `CHUNK_MAX` | 16 / 64 / 256 KiB | `rill-core` `blobs/chunking.rs` |

`BlobManifest { total_size, chunks: Vec<ChunkRef { hash, size: u32 }> }` is
`Encodable` and hashed under `BlobManifest`. Chunks and blobs are raw bytes
hashed under their own kinds.

`ContentRef` accessors:

* `size()`: the content length in bytes.
* `content_hash()`: a dedup key for any form; the blob or manifest hash, or
  `digest(Blob, bytes)` for inline content. Search and blame key on it.
* `stored_hash()`: the `(kind, hash)` to fetch, `None` for inline.

## Write path

`store_content(objects, bytes)`:

1. At or below `INLINE_THRESHOLD`, return `Inline` without touching the
   store.
2. Below `CHUNK_THRESHOLD`, `put_blob` and return `Blob { hash, size }`.
3. Otherwise `chunk_boundaries` runs the `fastcdc` crate's v2020 chunker on
   a blocking thread and
   returns `(offset, length)` cuts covering the input. Each chunk is a slice
   of the original `Bytes` (no copy), hashed under `Chunk` and put in turn.
   The manifest is then stored with `put_object` and returned as
   `Chunked { hash, size }`.

Every `put` verifies the hash (see
[storage and keyspace](storage-and-keyspace.md)), and a chunk that already
exists is a no-op. An edit in the middle of a large file therefore stores
only the chunks whose boundaries moved.

`store_content_stream(objects, parts)` does the same for a stream of byte
parts in bounded memory. Content below `CHUNK_THRESHOLD` is buffered and
stored whole; past it, the parts feed `fastcdc`'s `AsyncStreamCDC` (through
`tokio_util`'s `StreamReader`) with the same parameters, which cuts exactly
where the one-shot chunker would, so the result is identical to
`store_content` on the whole content (a test checks this). It hashes the
bytes as they pass,
returns `StoredContent { content, blob_hash, size }`, and fails with
`Error::Upload` past 5 GiB. Content that ends up under `CHUNK_THRESHOLD`
falls back to `store_content`. A 1 GiB upload grows resident memory by less
than 64 MiB (`tests/streaming_memory.rs`).

Producers: the server's `WriteBlob` RPC reads the header message, checks
write permission, streams the rest through `store_content_stream`, then
checks the announced `size` and optional `expected_hash` (the `Blob`-kind
hash of the bytes, whatever form is chosen) against the result. The SDK's
`store_content` inlines at or below `INLINE_THRESHOLD` and otherwise sends
`WriteBlob`, so chunking is always decided on the server. The change builder,
text merge and patch paths in `rill-core` and `rill-api` call
`store_content` directly.

On `PutChange` the server checks each `Set` content with
`check_content_exists`: the referenced blob or manifest must be present, and
inline content must not exceed `MAX_INLINE` (1 MiB).

## Read path

`read_content(objects, content)`:

| Form | Steps | Errors |
| --- | --- | --- |
| `Inline` | clone the bytes | none |
| `Blob` | `get_required(Blob, hash)`, check length | `ObjectNotFound`, `Error::ContentSize` |
| `Chunked` | load the manifest, one `get_many` over all chunks, concatenate, check each chunk against `ChunkRef.size` and the total against `size` | `ObjectNotFound` for manifest or chunk, `ContentSize` |

The invariant readers rely on: `read_content` returns exactly
`ContentRef::size()` bytes or fails. The manifest's `total_size` must equal
the `ContentRef` size and the chunk sizes must sum to it. Every object read
is re-hashed by `VerifiedObjectStore`. `BlobReader` is the trait form,
implemented for `Arc<dyn ObjectStore>`, so merge and blame code can take any
reader.

Range reads touch only what they need. `read_content_range(objects,
content, offset, len)` returns the requested bytes, fetching only the
chunks that cover them. `content_parts` returns the same range as a
`'static` stream of parts, reading up to 16 chunks concurrently. `ReadBlob`
streams `content_parts` in 256 KiB messages (`BLOB_CHUNK`), so serving any
range of any file needs memory for a few chunks, not the file.

## Limitations

* `read_content` and `store_content` still hold whole content in memory;
  merge, diff and the SDK use them. Only `WriteBlob` and `ReadBlob` stream.
* Chunks are put one at a time, so a large file costs one sequential store
  round-trip per chunk.
* A failure part-way through chunking leaves orphan chunks with no manifest;
  nothing removes them.
* The 256-byte inline rule is a producer convention. The server accepts
  inline content up to `MAX_INLINE` (1 MiB), so equal bytes can appear as
  both `Inline` and `Blob` and only `content_hash()` matches them.
* The manifest has no per-chunk offsets; a range read sums chunk sizes to
  find its first chunk.

## Code

* [rill-core blobs/](../../crates/engine/core/src/blobs/mod.rs): `chunking.rs` (chunk sizes), `reader.rs` (`BlobReader`), `store.rs` (`store_content`, `chunk_boundaries`), `stream.rs` (`store_content_stream`), `read.rs` (`read_content`, `read_content_range`, `content_parts`)
* [rill-types objects/blob.rs](../../crates/foundation/types/src/objects/blob.rs): `ContentRef`, `ChunkRef`, `BlobManifest`, thresholds
* [rill-store object.rs](../../crates/foundation/store/src/object.rs): `put_blob`, `get_many`, `verify`
* [rill-api services/content/files.rs](../../crates/server/api/src/services/content/files.rs): `WriteBlob`, `ReadBlob`, `check_content_exists`, `MAX_INLINE`
* [rill-sdk repo/blobs.rs](../../crates/clients/sdk/src/repo/blobs.rs): client-side `store_content` and `put_blob`

---
← Previous: [Merge algebra](merge-algebra.md) · [Index](../index.md) · Next: [Derived data](derived-data.md) →
