# 002 · File content lives only in the object store

Status: accepted

## Context

Content (blobs, chunks, manifests, trees, change versions, large derived
values) is immutable and addressed by hash. It dominates storage volume and
read traffic, and it must be cacheable at node and regional tiers and
fetchable over plain HTTP.

## Decision

All content is stored in an S3-compatible object store, keyed by
`(kind, BLAKE3 hash)`. The metadata store holds only pointers and small
records. Large derived values are stored as objects with a metadata pointer.

## Consequences

* Objects are written before the metadata that references them; GC grace
  periods guarantee referenced objects outlive backups.
* Every tier verifies hashes on read, so caches can never serve wrong bytes.
* Object fetches need an authorization story independent of the metadata
  store: signed read grants.

## Alternatives rejected

* **Content in FoundationDB with a splitting layer:** violates FDB's value
  and transaction limits at scale and loses HTTP addressability.

## Revisit when

An object-store vendor cannot meet latency targets even behind the cache
tiers.
