# 018 · Working copies belong to a per-host daemon

Status: accepted

## Context

ADR 006 makes the server workspace authoritative and the local checkout a
cache with an overlay of edits. v1 kept that cache in each checkout's
`.rill/` (a TOML manifest rewritten on every operation, a full walk for
status, one blob fetch per file per checkout), which cannot serve a
10M-file repository or 100 agents on one host.

## Decision

* One daemon per user and host (`rilld`, the `rill-daemon` crate) owns
  every working copy, the object cache and the server sync. Tools talk to
  it over `rill.daemon.v1` on a Unix socket, with the same connectrpc
  stack as the server; the CLI starts it on demand. No tool keeps checkout
  state of its own, and the working tree holds only a marker.
* The overlay is the latest operation per path in redb, written
  durably before it is visible; status compares it with the base.
* Edits reach the server workspace as snapshots the server builds from
  edits, conditional on the workspace's etag, so clients never build or
  upload trees.
* Eager mounts (files on disk, a `notify` watcher) come first because
  they work on every OS; a Linux FUSE view (`fuser`) and read-only
  snapshot mounts follow. EdenFS (GPL) is a design reference only.

## Consequences

* Status and commit cost the number of local edits.
* A host fetches each object once for all its checkouts.
* Every local command depends on a running daemon (started on demand).
* The SDK no longer has a working copy; agents without files use the
  workspace API directly.

## Alternatives rejected

* **State in each checkout (v1):** O(files) status, per-checkout caches,
  state the repository's content could collide with.
* **An append-only journal file:** needs its own compaction and torn-write
  handling; redb gives both.
* **`interprocess` sockets:** the client (`Http2Connection::lazy_unix`)
  and axum already speak Unix sockets; Windows named pipes can be added
  when Windows gets a VFS.

## Revisit when

A platform VFS needs a different process model (macOS FSKit extensions),
or hosts need a shared multi-user cache.
