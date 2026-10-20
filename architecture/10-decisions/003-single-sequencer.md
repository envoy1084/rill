# 003 · One sequencer per repository; trunk is never sharded

Status: accepted

## Context

A single numbered trunk (`trunk@N`) is the product: an atomic, totally
ordered history that review, verification, bisect and revert all depend on.
At peak the target workload lands 20 changes per second into one
repository, with heavy contention on hot files.

## Decision

Each repository's trunk has exactly one writer, the sequencer, elected by a
lease with epoch fencing. Throughput comes from speculation (parallel
rebases onto predicted heads), conflict domains and group commit (many
landings per transaction), not from partitioning trunk.

## Consequences

* The trunk head is a single hot key written only by the sequencer, a few
  times per second.
* Changes spanning any set of directories land atomically.
* The sequencer must never block on long-running verification; pre-land
  checks are bounded and full verification runs post-land.

## Alternatives rejected

* **Per-directory trunks or sharded sequence spaces:** break atomic
  cross-directory changes and the meaning of `trunk@N`.
* **Lock-free multi-writer landing with CAS retries (v1):** collapses into
  retry storms under contention.

## Revisit when

The landing throughput gate (phase 17 of the roadmap) fails by more than 2×
after two iterations of tuning.
