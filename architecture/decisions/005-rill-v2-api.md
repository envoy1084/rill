# 005 · `rill.v2` is a clean break, served as gRPC and Connect

Status: accepted

## Context

`rill.v1` grew RPC by RPC: lists without pagination, no idempotency
standard, inconsistent resource naming and errors without machine-readable
reasons. Nothing is launched, so there are no external clients to preserve.
Browsers and many languages need first-class access.

## Decision

Design `rill.v2` from scratch following a subset of Google's API
Improvement Proposals (resource names, standard methods, pagination,
idempotency, etags, typed error reasons). Serve it as gRPC and as the
Connect protocol from the same handlers. `rill.v1` is deleted once every
client moves.

## Consequences

* The web UI and any HTTP client use the same API as the SDKs; there is no
  separate REST gateway that could drift.
* `buf lint` and `buf breaking` enforce the rules once `v2` is published.

## Alternatives rejected

* **REST/JSON gateway in front of gRPC:** a second API surface.
* **Evolving v1 in place:** carries every v1 shape problem forward.

## Revisit when

Never for v2; a v3 needs its own ADR.
