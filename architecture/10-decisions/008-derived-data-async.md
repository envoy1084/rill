# 008 · Derived data is asynchronous except a land-critical set

Status: accepted

## Context

Root trees, generation numbers, changed paths, blame, path history and
search indexes are all pure functions of a version. Computing all of them
on commit makes writes slow; computing all of them on demand stalls the
first reader.

## Decision

A small land-critical set (root tree, skeleton, changed paths, generation)
is computed in the commit path and must exist before a version can land.
Everything else is computed by background jobs and on demand as a fallback.

## Consequences

* Commit latency stays O(change).
* Readers of heavy derived data may observe lag, exposed explicitly (for
  example `indexed_through` on search).

## Alternatives rejected

* **Everything synchronous:** slow writes. **Everything lazy (v1):** cold
  stalls and unpredictable latency.

## Revisit when

A derivation outside the set becomes necessary for landing decisions.
