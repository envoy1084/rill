# 020 · The services ship as their own binary, `rill-server`

Status: accepted

## Context

`rill` was one binary with every role: the client, the client daemon, and
the services (`rill dev`, `rill api`, `rill cache`). A developer installing
the client downloaded the lander, the policy engine, execution workers,
the identity services and the cloud storage and key clients: 169 MB with
line tables, which no client runs, and an attack surface no client needs.
The services already ship as a container image, built on their own
schedule.

## Decision

* Three binaries:
  * `rill`: the CLI and the client daemon. The daemon stays in the same
    binary (`rill daemon run`, started by the CLI running itself), so the
    two can never be different versions and a checkout needs one file.
  * `rill-server`: `run`, `dev`, `cache`, `init-config`, `mint-token` and
    `import git --storage-dir`, with the deployment features.
  * `rill-mcp`: unchanged.
* `rill dev` stays as a convenience: it finds `rill-server` next to `rill`
  or on `PATH` and replaces itself with `rill-server dev`.
* The client crates never link the server crates; CI checks `rill-cli`'s
  and `rill-mcp`'s dependency trees.
* No compatibility: `rill api` and `rill cache` are gone, not aliased.

## Consequences

* The client no longer carries the services' code. Structural-merge
  grammars stay in it: the daemon merges files when a checkout syncs.
* Two artifacts to release instead of one; `rill dev` needs both on a
  laptop.
* Deployment configuration (image, chart, compose) names `rill-server`.

## Alternatives rejected

* **A separate `rilld` daemon binary:** version skew between CLI and
  daemon, a second file to install and find, and little saved, since the
  daemon shares most of its code with the CLI.
* **Cargo features on one binary:** two builds of one crate with
  different behaviour, and nothing to stop a client build from linking
  the services again.

## Revisit when

A client needs to run a service role locally beyond `dev`, or the merge
grammars move out of the client.
