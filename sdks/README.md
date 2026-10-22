# SDKs

Clients for the `rill.v1` API in Go, TypeScript and Python: the generated
Connect clients plus a thin layer (a client holding every service, names,
error reasons, retries). The Rust SDK is `crates/clients/sdk`. See
[SDKs in other languages](../architecture/08-clients/other-sdks.md).

* `generate.sh` regenerates every language's code and the API reference
  from the protos.
* `conformance/run.sh [rust] [go] [ts] [python]` starts `rill dev` and runs
  the shared [conformance scenarios](conformance/README.md).
