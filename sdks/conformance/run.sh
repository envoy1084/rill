#!/usr/bin/env bash
# Run the SDK conformance scenarios against a fresh `rill dev`: the Rust
# suite in-process, then the Go, TypeScript and Python suites against the
# server this script starts. Pass languages to run only those
# (`run.sh go ts`).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
langs=${*:-rust go ts python}
api=${RILL_CONFORMANCE_PORT:-17470}
http=$((api + 1))

cargo build -q -p rill-server --manifest-path "$root/Cargo.toml"
"$root/target/debug/rill-server" dev --memory \
  --listen-grpc "127.0.0.1:$api" --listen-http "127.0.0.1:$http" \
  >"${TMPDIR:-/tmp}/rill-conformance.log" 2>&1 &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
for _ in $(seq 100); do
  curl -fsS "http://127.0.0.1:$http/readyz" >/dev/null 2>&1 && break
  sleep 0.2
done
export RILL_ENDPOINT="http://127.0.0.1:$api"

for lang in $langs; do
  echo "== $lang"
  case $lang in
    rust) cargo test -q -p rill-sdk --features testing --test conformance \
      --manifest-path "$root/Cargo.toml" ;;
    go) (cd "$root/sdks/go" && go test -count=1 -v ./rill | grep -E "^(--- |ok|FAIL)") ;;
    ts) (cd "$root" && pnpm install --frozen-lockfile --silent && pnpm --filter @rill/sdk test) ;;
    python) (cd "$root/sdks/python" && uv run --frozen pytest -q) ;;
    *) echo "unknown language $lang" >&2; exit 2 ;;
  esac
done
