# 016 · Raw content is plain BLAKE3

Status: accepted

## Context

[ADR 009](009-blake3.md) kept domain-separated BLAKE3 for every object,
`BLAKE3("rill:" ++ kind ++ ":" ++ bytes)`, and claimed execution needed no
rehashing. That was wrong: REAPI clients (Bazel's
`--digest_function=blake3`, Buildbarn) name a file by BLAKE3 of its bytes
alone, so the same file had two ids, and the execution CAS
([ADR 007](007-reapi-execution.md)) could not be Rill's object store
without storing content twice or keeping a mapping table.

## Decision

Raw content, the `Blob` and `Chunk` kinds, is hashed as plain BLAKE3 of
its bytes. Structured kinds (manifests, trees, change versions, derived
objects) keep the `"rill:" ++ kind ++ ":"` separator. This supersedes ADR
009 for raw content only.

## Consequences

* A file in a repository is already in the execution CAS under the digest
  a build tool asks for; inputs and outputs deduplicate with repository
  content inside each org.
* Raw bytes can no longer be told from structured objects by hash alone.
  Objects are always addressed by `(kind, hash)` and stored under their
  kind, and a structured hash never equals a raw one for the same bytes,
  so nothing can be read as the wrong kind.
* Pre-release, the change is made in place; existing data is recreated.

## Alternatives rejected

* **A separate CAS kind keyed by plain BLAKE3, with a mapping:** every
  repository file stored twice for builds, or rehashed when an input root
  is built from a tree.
* **Plain BLAKE3 for every kind:** loses the guarantee that an encoded tree
  or version cannot pass for another structured kind.

## Revisit when

REAPI clients move to a digest function Rill does not use, or a weakness
in BLAKE3 is published.
