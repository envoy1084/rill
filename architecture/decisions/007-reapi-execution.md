# 007 · Execution speaks REAPI v2 with BLAKE3

Status: accepted

## Context

Verification is the slowest part of landing and, in v1, runs repository
commands as the server user. Large monorepos already use Bazel, Buck2 or
Pants, which speak the Remote Execution API (REAPI). Bazel supports BLAKE3
as the REAPI digest function since 6.4.

## Decision

Rill's execution service implements REAPI v2 with BLAKE3 digests, using the
object store as its content-addressable storage, on a sandboxed worker
fleet. Repositories without a build tool declare steps in
`.rill/verify.toml`, compiled to REAPI actions.

## Consequences

* One action cache shared by agents, pre-land checks, post-land
  verification and developers' builds.
* Repository code only ever runs in sandboxes on workers, never in the API
  or sequencer processes.

## Alternatives rejected

* **A proprietary execution protocol:** every build tool would need an
  adapter.

## Revisit when

A dominant build tool in adopters' repos cannot use REAPI.
