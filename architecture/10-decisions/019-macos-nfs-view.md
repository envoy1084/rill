# 019 · The macOS lazy view is an NFS loopback server

Status: accepted

## Context

ADR 018 puts working copies behind a per-host daemon with lazy views.
Linux has FUSE. macOS has no FUSE in the kernel: macFUSE and FUSE-T are
commercially licensed, and FSKit (macOS 15+) needs an app extension and
is not ready for a writable file system of this kind.

## Decision

* The view is platform-neutral (`vfs/` in `rill-daemon`); front ends are
  thin bridges onto it.
* On macOS the daemon serves the view as NFSv3 on 127.0.0.1, one server
  per mount (`nfsserve`, BSD-3-Clause), and macOS's own NFS client
  mounts it with `mount_nfs`. A user may mount onto a directory they
  own, so no privileged helper is needed.
* NFS is stateless: writes are buffered per file and committed when the
  file goes quiet, before namespace changes, and when the mount settles.

## Consequences

* No kernel extension, driver or install step on macOS.
* Reads always go through the daemon (no passthrough); they are served
  from the cached object on local disk.
* File handles do not survive a daemon restart; the mount is remounted.

## Alternatives rejected

* **macFUSE, FUSE-T:** commercial licences.
* **FSKit:** app extension packaging and not ready for this use.
* **Eager mode only on macOS:** the size of a broad checkout of a large
  repository.

## Revisit when

FSKit supports a writable network-style file system without an app
bundle, or an unprivileged mount is no longer allowed.
