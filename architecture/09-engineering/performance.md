# Performance

Two tools measure Rill's speed. `rill-bench` times the hot algorithms on a
deterministic generated repository, to show that cost tracks the size of a
change and to catch regressions. `rill-load` drives a running server with
synthetic agents at a chosen fraction of the D1 design workload (the target
of 30,000 agents and 1,000 humans on one repository) and reports throughput
and latency per operation.

## Benchmarks

`rill-bench` builds a repository from a `Shape`: file count, directory
fan-out 32, 16 lines per file, fixed seed. The same shape always produces
the same files and trees. Benchmarks in `benches/core.rs`:

| Benchmark | 100k files | 1M files |
| --- | --- | --- |
| `commit_10_files` (build, store, derive root) | 0.57 ms | 0.82 ms |
| `diff_adjacent_roots` (10 files changed) | 0.21 ms | 0.31 ms |
| `rebase_10_files_over_20` | 0.23 ms | 0.33 ms |
| `diff_lines_2000` | 58 µs | 60 µs |
| `merge_file_2000` | 127 µs | 183 µs |

Baselines are from an Apple M-series laptop, release build, memory stores.
Ten times the files adds one directory level, not ten times the work:
commits, tree diffs and rebases are O(size of change). Building the 1M-file
corpus takes about a minute and 4.5 GB of memory.

```
cargo bench -p rill-bench --bench core
RILL_BENCH_FILES=1000000 cargo bench -p rill-bench --bench core
cargo bench -p rill-bench --bench meta
cargo bench -p rill-bench --bench trees
RILL_BENCH_FILES=1000000 cargo bench -p rill-bench --bench trees
```

`benches/trees.rs` times a one-file commit, a full walk and a one-file
diff in the deep corpus, and commits, lookups and diffs in one
200,000-entry directory, printing the tree nodes each one-file commit
writes; the numbers are in [trees](../03-core-algorithms/trees.md#scale).
The corpus is built as a chain of 5,000-file commits, since one change is
capped at 8 MiB.

`benches/meta.rs` times metadata round trips (conditional write, point
read, 100-row scan) on the memory and redb engines; the numbers are in
[metadata engines](../02-data/metadata-engines.md#performance).

## Load

```mermaid
flowchart LR
  P[Profile: % of D1] --> T1[reads ticker] & T2[versions ticker] & T3[review ticker] & T4[land ticker]
  T1 & T2 & T3 & T4 --> L{in-flight limit}
  L -->|permit| W[Workload over rill-sdk]
  L -->|full| D[dropped]
  W --> S[Server: embedded dev or --endpoint]
  W --> H[HDR histograms per op]
```

Each workload class has an open-loop ticker at its target rate. 100% of D1
is 50,000 reads, 35 new versions, 23 votes and 1.16 landings a second.

- **Reads** are 50% file reads, 20% directory listings, 15% grep and 15%
  log.
- **Versions** edit one file at trunk. A share (`--hot-share`) goes to five
  hot files.
- **Reviews** submit a draft and approve it.
- **Landings** request one and wait for the outcome; that latency is
  queue-to-trunk.

When the in-flight limit is reached, new work is counted as dropped rather
than queued, so overload shows in the report instead of inflating latency.

```
cargo run --release -p rill-load -- --percent 1 --duration 60
cargo run --release -p rill-load -- --endpoint http://host:7470 --percent 5 --json
```

Laptop results against the embedded single-process server:

| Load | Reads/s achieved | Read p99 | Errors |
| --- | --- | --- | --- |
| 1% of D1 | 500 | ≤ 2.5 ms | 0 |
| 5% of D1 | 2,500 | ≤ 2.3 ms | 0 |

Land latency is dominated by the lander's 2-second batch window.

### On FoundationDB

A release `rill-api` with metadata in a local single-node
FoundationDB and `rill-load --percent 10` (3.5 commits, 5,000 reads a
second) on one laptop:

| Objects | Commit p50 | Commit p99 | Reads p99 |
| --- | --- | --- | --- |
| in memory | 22 ms | 30 ms | 2.4 ms |
| local files | 41 ms | 318 ms | 41 ms |

A commit is a handful of FoundationDB transactions (claim, commit,
response) plus object writes. With local files the tail is the object
store's fsync per file (macOS flushes the drive on every one), not the
metadata. At 50% of D1 the one laptop running load, server and database
saturates: reads are dropped and every latency rises, commits to 384 ms
p99.

## Limitations

- No benchmark covers search indexing, blame or path history yet.
- At low percentages the land ticker fires rarely, so a short run gives few
  land samples.
- The load harness creates its own repository. It cannot load an existing
  one.
- Results come from one machine. There is no tracked history of numbers and
  no regression gate.

## Code

- [rill-bench/src/lib.rs](../../tools/bench/src/lib.rs), [rill-bench/benches/core.rs](../../tools/bench/benches/core.rs), [rill-bench/benches/meta.rs](../../tools/bench/benches/meta.rs)
- [rill-load/src/run.rs](../../tools/load/src/run.rs), [workload.rs](../../tools/load/src/workload.rs), [profile.rs](../../tools/load/src/profile.rs), [stats.rs](../../tools/load/src/stats.rs)

---
← Previous: [Simulation](simulation.md) · [Index](../index.md) · Next: [Architecture decisions](../10-decisions/README.md) →
