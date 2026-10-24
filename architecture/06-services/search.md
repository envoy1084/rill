# Search

`rill-search` serves code search from immutable index segments, in the
style of Zoekt and Lucene. Three things make it scale:

- **Segments are keyed by content.** A blob is tokenized once, however
  many roots or paths hold it.
- **Path maps are layered.** A landing costs what it changed.
- **Shards are searched in parallel.** Each shard is a list of segments.

```mermaid
flowchart LR
  L[landing] -->|index job| W[writer: path layer + new segments]
  W --> S[(segment objects)]
  W --> R[(idx:segments@1 / idx:paths@1)]
  Q[Grep] --> P[planner: trigram query]
  P --> F[every shard in parallel]
  F --> V[candidates held by the root at permitted paths]
  V --> C[verify on stored content]
  C --> K[rank, facets]
```

## Segments

A segment ([segment/](../../crates/domain/search/src/segment/mod.rs)) is an
immutable object (`ObjectKind::Derived`) covering a batch of blobs. It
holds:

- **A trigram dictionary:** an [`fst`](https://crates.io/crates/fst) map from
  each three-byte sequence (ASCII case-folded) to its posting list.
- **Posting lists:** roaring bitmaps of the segment's document numbers.
- **Each blob's content,** compressed with zstd, so a candidate is verified
  without fetching the blob.
- **Each blob's symbols:** definitions extracted with sem-core, using the
  first path the blob was indexed under to pick the language.

Segments never change. A blob over 1 MiB or binary is not put in a segment
(its marker says "skipped"), so it is findable by path only.

Blobs are spread over `shards` (16 by default) by content hash.

**Where segments live.** With `[search] cache_dir` set, a server keeps
segments on local disk, under a subdirectory per repository, and
memory-maps them with `memmap2`. Otherwise it holds them in memory. Either
way they are fetched from the object store once.

## Path layers

Which files a root has is a [`PathLayer`](../../crates/domain/search/src/paths/layer.rs),
stored per root (`idx:paths@1`):

- **`Base`:** every file of the root, with its content hash and kind.
- **`Delta`:** the parent root's files with this root's changes applied.
  The changes come from `diff_trees`, so a landing costs what it changed.

A new base is written when a chain reaches 32 deltas, or when a change
touches more than 50,000 files.

A [`PathView`](../../crates/domain/search/src/paths/view.rs) is a base map
plus the overlay of every delta above it, cached per process. It answers:

- which paths hold a content hash;
- what is at a path;
- which paths changed recently (a ranking signal).

## Writing the index

One server writes, the holder of the repository's `search` lease; every
write is fenced on it.

Each landing queues an index job for the new trunk head. The worker also
queues the current head when it starts, because trunk can move without a
landing (imports). `index_version`:

1. Takes the version's root and its first parent's root.
2. Writes a delta when the parent is indexed and the chain is short, a
   full walk otherwise.
3. Reads only the blobs no marker covers, prepares them in parallel off
   the async threads (tokenize, compress, extract symbols), and fills one
   builder per shard. A builder is cut into a segment at `segment_mb` of
   content.
4. Commits in this order:
   - the segment list (`idx:segments@1`, with a generation);
   - the blob markers (`idx:marker@1`);
   - the root's layer.

   A reader never sees a layer whose blobs are not in a listed segment.
5. Records `indexed_through` (trunk position and root) when the version is
   on trunk.

After each job, `compact` merges segments tier by tier. Once a shard has
`merge_fanout` segments of one level, they become one segment of the next
level, built from the stored contents (no blob is fetched again). The
segment list then swaps them in one write.

Every other server reloads when the stored generation moves. It costs one
read per search and opens only new segments.

## Queries

**Planning.** The planner
([trigram/plan.rs](../../crates/domain/search/src/trigram/plan.rs)) is a
port of Russ Cox's codesearch analysis onto `regex-syntax`'s HIR:

- Each sub-expression is summarised by whether it can match empty, its
  exact strings (when few), its prefixes and suffixes, and a trigram query.
- Concatenation crosses exact sets, so `(abc|def)ghi` requires `abcghi` or
  `defghi`, and `x(a|b)yz` requires `xayz` or `xbyz`.
- Oversized sets fold into the query.
- The query is sound (a property test checks this): any document the regex
  matches satisfies it.

**Search.**
[`search(index, scope, query, allow)`](../../crates/domain/search/src/query/search.rs)
runs every shard on the blocking pool at once. For each segment, the
candidates are documents that:

1. satisfy the trigram query (all documents when the pattern has no
   required trigram);
2. are held by the scope's view at some path;
3. are at a path the glob, the language filter and the caller's
   `PathFilter` admit (the filter is applied after intersection, from the
   current permissions, so nothing about access is baked into segments);
4. are not redacted (the object store's `refuses`).

Candidates are then verified with the regex (16 MiB size limits) on the
stored content. A shard stops at twice the requested hits (at least 50
files), or when the query's time budget runs out (`partial`).

**Ranking (v1).** Files are scored, then hits flattened best file first:

| Signal | Score |
| --- | --- |
| base | 100 − 3 per directory level |
| the file name matches | +20 |
| a matched line defines a symbol | +25 |
| under `src/` or `lib/` | +15 |
| changed in the view's recent deltas | +10 |
| under `vendor/`, `third_party/`, `node_modules/`, `generated/` | −30 |
| a test file | −15 |

Facets count matched files by top-level directory and by language.

## Serving shards across servers

By default every server serves every shard. With `[search] serve_shards`
set, a server opens only those shards' segments ahead of queries, and
[`SearchNodes`](../../crates/server/runtime/src/state/search_nodes.rs)
heartbeats its advertised URL and shard list into one record per
repository (`search-nodes`, every 10 s, entries live 30 s).

A `Grep` on any server then:

1. builds the scope and searches its own shards plus the live files;
2. sends each other shard to a live server serving it (spread by shard
   number) with `ContentService.GrepShards`, pinning the revision to a
   version hash or `tree:<hash>` and forwarding the caller's
   `authorization` header, so the peer filters by the caller's access;
3. searches a shard itself when no live server serves it, or when the
   peer fails or misses the time budget plus 5 s;
4. merges the ranked files of every part and flattens them
   ([`assemble`](../../crates/domain/search/src/query/search.rs)).

The advertised URL is `advertise_url`, else the bound gRPC address
(`https` when the server has TLS). On a wildcard address set
`advertise_url`. Peers over `https` are verified against the system roots
plus `peer_ca_file`.

## Structural search

With `structural`, the pattern is an [ast-grep](https://ast-grep.github.io)
pattern (`foo($A, $$$REST)`), matched on syntax trees by `ast-grep-core`
([structural/](../../crates/domain/search/src/structural/mod.rs)):

- it needs exactly one language: Rust, Go, Python, JavaScript, TypeScript
  (TSX for `.tsx`), Java, C, C++ or Ruby;
- the grammars are the tree-sitter versions sem-core pins (tree-sitter
  0.26), which is why `ast-grep-core` is pinned to 0.45;
- in languages where `$` cannot start an identifier, meta-variables are
  rewritten to `µ` before parsing, as ast-grep does;
- candidates are narrowed by the trigrams of the pattern's identifiers and
  literals that are not meta-variables, then each candidate is parsed.

## Symbols and SCIP

**Extracted symbols.** Each blob's definitions (sem-core) are stored in its
segment. [`find_symbols`](../../crates/domain/search/src/symbols/lookup.rs)
looks names up exactly or by substring: the trigram index narrows the blobs,
then their stored symbols are checked. `file_symbols` reads one file's.

**SCIP.** A repository's CI can upload a [SCIP](https://github.com/sourcegraph/scip)
index (`scip` crate) for a revision, for example from `rust-analyzer scip`,
`scip-typescript` or `scip-python`:

1. the client writes the encoded index with `WriteBlob`, then calls
   `UploadScip` with the revision and a path prefix;
2. the server keeps each global symbol's occurrences in the revision's
   files, with each file's content hash
   ([scip/](../../crates/domain/search/src/scip/mod.rs)), as a derived
   object named by `idx:scip@1` for that root and as the latest upload;
3. a lookup uses the root's own upload, else the latest, trusting a file's
   occurrences only while the scope still holds the same content.

**RPCs.**

| RPC | Answers |
| --- | --- |
| `ListSymbols` | a file's symbols, or symbols whose names contain `query` |
| `FindDefinitions` | SCIP definitions, then extracted definitions in other files |
| `FindReferences` | by name or file position: SCIP occurrences when a symbol is covered (`precise`), else whole-word matches with definitions marked |
| `UploadScip` | attaches an index; needs write access somewhere |

## Scopes and freshness

A [`Scope`](../../crates/domain/search/src/query/scope.rs) is a view plus
**live files**: files read from the object store and scanned directly,
which shadow their paths in the view.
[`RepoContext::grep`](../../crates/server/runtime/src/state/grep.rs)
builds one per request:

1. **An indexed root:** its view.
2. **A version that isn't indexed** (an unlanded change, a workspace
   snapshot, a just-landed head the writer has not reached): its nearest
   indexed first-parent ancestor (up to 64 back). The view gets the diff
   applied, and the changed files are read live (up to 2,000).
3. **Otherwise:** the writer is asked to index the root, and the search
   waits up to 30 s. Concurrent searches of one root share one job
   (`state::index_requests`).

Every response carries `indexed_through`. A request with `min_seq` waits
up to 30 s until trunk is indexed through that position, then fails with
`UNAVAILABLE`.

## Server surface

`ContentService.Grep` takes:
- a pattern (regex, `literal`, or `structural`), case-insensitivity, path
  globs and `languages`;
- `max_results` (default and cap 5,000) and `context_lines` (cap 20);
- the revision, and `min_seq`.

It returns:
- ranked hits;
- `truncated`, `partial`, `files_matched`;
- the directory and language facets;
- `indexed_through`.

It requires read access somewhere; results are filtered to readable paths.
`GrepShards` (one server's part of a search, for peers) needs the same.

| `[search]` | Default | Meaning |
| --- | --- | --- |
| `shards` | 16 | shards blobs are spread over |
| `cache_dir` | none | local segment cache (memory when unset) |
| `segment_mb` | 64 | content per fresh segment |
| `merge_fanout` | 8 | segments per tier before a merge |
| `time_budget_ms` | 10000 | when a query returns partial results |
| `serve_shards` | all | shards this server serves |
| `advertise_url` | bound address | where peers reach this server |
| `peer_ca_file` | none | extra CA for `https` peers |

Error mapping:
- `InvalidPattern`, `InvalidGlob` and `InvalidScip` are `INVALID_ARGUMENT`;
- `RootNotIndexed` and an unmet `min_seq` are `UNAVAILABLE`;
- `Codec` and `Join` are `INTERNAL`.

## Limitations

- Merged-away segment objects stay in the object store until GC (phase 28).
- A blob's symbols and language come from the first path it was indexed
  under.
- Case-insensitive planning folds ASCII only.
- A redacted blob is hidden at query time, but its segment still holds the
  content until it is merged or rebuilt.
- Symbol lookups and references run on the server that receives them; they
  open shards it does not serve rather than fanning out.
- A SCIP index is held in memory whole while in use (the last four).

## Code

* [segment/](../../crates/domain/search/src/segment/mod.rs): format, writer, reader, merge
* [paths/](../../crates/domain/search/src/paths/mod.rs): layers and views
* [index/](../../crates/domain/search/src/index/mod.rs): records, cache, writer, compaction, snapshot
* [query/](../../crates/domain/search/src/query/mod.rs): scope, search, ranking, facets
* [trigram/](../../crates/domain/search/src/trigram/mod.rs): extraction, planner, queries
* [structural/](../../crates/domain/search/src/structural/mod.rs): ast-grep grammars and patterns
* [symbols/](../../crates/domain/search/src/symbols/mod.rs): extraction and lookups
* [scip/](../../crates/domain/search/src/scip/mod.rs): SCIP parsing, storage and views
* Server: [services/content/symbols.rs](../../crates/server/api/src/services/content/symbols.rs), [state/grep.rs](../../crates/server/runtime/src/state/grep.rs), [state/search_nodes.rs](../../crates/server/runtime/src/state/search_nodes.rs), [services/content/peers.rs](../../crates/server/api/src/services/content/peers.rs), [workers/indexer.rs](../../crates/server/runtime/src/workers/indexer.rs), [services/content/grep.rs](../../crates/server/api/src/services/content/grep.rs)

---
← Previous: [Security review packet](../05-policy/security-review.md) · [Index](../index.md) · Next: [Exec](exec.md) →
