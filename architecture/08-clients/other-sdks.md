# SDKs in other languages

`sdks/` holds clients for Go, TypeScript and Python, generated from the
same protos as the server and the [Rust SDK](sdk.md), with a thin layer
of their own. They are not published yet. Each is the generated Connect
client for every service plus four small modules, the same in each
language:

| Module | Go (`sdks/go/rill`) | TypeScript (`sdks/ts/src`) | Python (`sdks/python/src/rill_sdk`) |
| --- | --- | --- | --- |
| A client holding every service, with an interceptor that sends the bearer token, a fresh `x-request-id` and a user agent on every call | `rill.New(endpoint, WithToken(…))` | `createRillClient(endpoint, {token})` | `rill_sdk.connect(endpoint, token=…)` (sync clients; the async ones are generated too) |
| Resource names from what callers hold (a bare repository name, a change id's bytes) and request ids | `names.go` | `names.ts` | `names.py` |
| The error's reason, what a policy denial misses, the requested wait | `rill.Reason`, `Is`, `Missing`, `RetryAfter` | `reason`, `isReason`, `missing`, `retryAfterMs` | `reason`, `is_reason`, `missing`, `retry_after` |
| Retrying while the cluster is unavailable or the server asks for a wait, within 20 s | `rill.Retry` | `retry` | `retry` |

The generators are pinned: `protoc-gen-go` and `protoc-gen-connect-go`
(Go), `protoc-gen-es` through `package-lock.json` (TypeScript, Connect-ES
2), and `protoc` 36.2 with `protoc-gen-connect-python` through `uv.lock`
(Python, connect-python). Each language has its `buf.gen.yaml`;
`sdks/generate.sh` regenerates all three and the [API
reference](../07-runtime/api-reference.md), and CI fails when the
committed code differs. Go and Python use the published `google.rpc`
messages (`genproto`, `googleapis-common-protos`); TypeScript generates
them.

## Conformance

`sdks/conformance/README.md` lists the scenarios every SDK runs:
resources and a `NOT_FOUND` reason, a write read back, an idempotent retry
and a reused request id, history pages, a reviewed change landing and a
filter finding it, and an etag guarding an update. `sdks/conformance/run.sh`
starts `rill dev --memory` and runs the Rust suite (in-process, through
`testing::DevServer`) and the Go, TypeScript and Python suites against
it; each language's suite skips its server scenarios without
`RILL_ENDPOINT`.

## Limitations

- Only the Rust SDK wraps workspaces, checkouts, uploads in chunks, read
  grants and sign-in; the others expose the generated calls for those.
- The thin layers speak the Connect protocol over HTTP/1.1, so a server
  stream holds one connection.

## Code

- [sdks/go/rill/](../../sdks/go/rill/client.go), [sdks/ts/src/](../../sdks/ts/src/index.ts), [sdks/python/src/rill_sdk/](../../sdks/python/src/rill_sdk/__init__.py)
- [sdks/generate.sh](../../sdks/generate.sh), [sdks/conformance/](../../sdks/conformance/README.md)

---
← Previous: [SDK](sdk.md) · [Index](../index.md) · Next: [Client daemon](daemon.md) →
