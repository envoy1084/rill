#!/usr/bin/env bash
# Regenerate everything derived from the protos: the API reference and the
# Go, TypeScript and Python SDKs' generated code. Needs buf, protoc,
# protoc-gen-doc, protoc-gen-go and protoc-gen-connect-go on the path; the
# TypeScript generator comes from the pnpm workspace's lockfile and the
# Python one from that SDK's.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
protos=crates/foundation/proto
google=$protos/proto/google

pnpm install --frozen-lockfile --silent
(cd sdks/python && uv sync --frozen --quiet)

buf lint "$protos"
(cd "$protos" && buf generate --exclude-path proto/google)

rm -rf sdks/go/gen sdks/ts/src/gen sdks/python/src/rill
buf generate "$protos" --template sdks/go/buf.gen.yaml --exclude-path "$google"
buf generate "$protos" --template sdks/ts/buf.gen.yaml
buf generate "$protos" --template sdks/python/buf.gen.yaml --exclude-path "$google"
