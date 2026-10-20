# 012 · Rust on the server; generated clients elsewhere

Status: accepted

## Context

The server, sequencer, workers, daemon and simulator must share one
implementation of the core algorithms and be testable together in a
deterministic simulator. Adopters write tooling in TypeScript, Python, Go and
Java.

## Decision

Every server-side component and the client daemon are written in Rust. SDKs
for other languages are generated from `rill.v2` protos with a thin
handwritten layer for sessions, retries and authentication.

## Consequences

* One simulator covers every component.
* Non-Rust SDKs never reimplement core algorithms (merge, diff); they call
  the API.

## Alternatives rejected

* **Mixed-language services:** splits the simulator and duplicates logic.

## Revisit when

Never for the server.
