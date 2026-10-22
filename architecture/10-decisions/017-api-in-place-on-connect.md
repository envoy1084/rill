# 017 · The API is redesigned in place as `rill.v1`, on connect-rust

Status: accepted

Supersedes the naming and transport of [005](005-rill-v2-api.md); its
design rules (AIP resource names, standard methods, pagination, etags,
typed error reasons, one handler set for gRPC and Connect) stand.

## Context

ADR 005 planned the redesigned API as a new `rill.v2` package beside
`rill.v1`, deleted once every client moved. Rill is pre-release: there are
no external clients, stored formats and the wire protocol change freely,
and nothing is kept for compatibility. Carrying two packages through the
redesign would mean an adapter and two sets of handlers for no one. The
server spoke gRPC only (tonic), so browsers and plain HTTP clients needed a
gateway.

## Decision

* The redesign happens in place: the package stays `rill.v1` and each
  service is rewritten to the ADR 005 rules. No `v2` names and no
  compatibility layer.
* The server and the Rust clients use **connect-rust** (`connectrpc`, the
  ConnectRPC project's implementation) with **buffa** messages: one router
  answers Connect (HTTP/1.1 or HTTP/2, binary or JSON), gRPC and gRPC-web.
  Code is generated from the protos at build time; pre-1.0 crates are
  pinned exactly.
* REAPI stays on tonic and prost behind the same listener: Bazel and the
  workers speak plain gRPC with the standard REAPI protos, which change
  only with the upstream API.
* SDKs for other languages are generated from the same protos with `buf`
  (connect-es, connect-go, connect-python), each with a small handwritten
  layer (ADR 012).

## Consequences

* Browsers, `curl` and the web UI call the API directly; there is no
  gateway that could drift.
* The redesign lands service by service with the whole workspace green
  after each, and no window in which two APIs are served.
* Two protobuf runtimes in one binary: buffa for `rill.v1`, prost for
  REAPI.
* Generated messages hold optional sub-messages as `MessageField` and
  enums as `EnumValue`; conversions to `rill-types` live in `rill-proto`.

## Alternatives rejected

* **A `rill.v2` package beside `rill.v1`:** two APIs and an adapter while
  nothing depends on the old one.
* **tonic plus a Connect/gRPC-web translation layer:** a second protocol
  stack to keep in step with the first.
* **Moving REAPI to connect-rust:** regenerating Google's protos with a
  second toolchain buys nothing; Bazel never speaks Connect.

## Revisit when

Rill is released: from then the API evolves additively and a breaking
change needs a new package and its own ADR. Reopen the transport if
connect-rust stops passing the ConnectRPC conformance suite or stalls.
