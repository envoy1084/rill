# 006 · The server workspace is authoritative; checkouts are caches

Status: accepted

## Context

v1 keeps working state in two places: the server workspace record and the
client's `.rill/checkout.toml`. They drift (a workspace still on the empty
tree while the checkout is on trunk), and handoffs between agents, humans
and CI cannot rely on either.

## Decision

The server workspace is the single source of truth for base, working change
and uncommitted snapshot. A local checkout is a cache of a workspace plus an
overlay journal of local edits, owned by the client daemon and synced to the
server continuously.

## Consequences

* Any machine can resume or take over a workspace.
* Client state lives outside the working tree, so repository content can
  never alter it.

## Alternatives rejected

* **Client-owned state (v1):** drift and no handoff.

## Revisit when

Offline-first operation becomes a goal (it is a non-goal for v2).
