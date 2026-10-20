# 009 · Keep domain-separated BLAKE3

Status: accepted

## Context

Object identity is `BLAKE3("rill:" ++ kind ++ ":" ++ bytes)`. BLAKE3 is
fast, parallel and tree-hashable, and REAPI clients support it. Git uses
SHA-1 or SHA-256.

## Decision

Keep domain-separated BLAKE3 for every object. Git interoperability stores
a per-object mapping between git ids and Rill ids.

## Consequences

* No rehashing for execution; git export and import maintain mapping
  tables.

## Alternatives rejected

* **SHA-256 everywhere:** slower, and no gain except closer git alignment,
  which the mapping table provides anyway.

## Revisit when

A cryptographic weakness in BLAKE3 is published.
