#!/bin/sh
# Start remote execution workers against a Rill server, locally: one
# privileged container runs its own Docker engine with gVisor, and inside
# it bb_worker (talking to Rill) and bb_runner (running actions under
# gVisor, without network).
#
#   RILL_WORKER_SECRET=... deploy/exec/run.sh         # start
#   deploy/exec/run.sh sample                         # build deploy/exec/sample remotely
#   deploy/exec/run.sh escape                         # the sandbox escape suite
#   deploy/exec/run.sh bazel build //...              # run Bazel against it in /src
#   deploy/exec/run.sh stop
#
# RILL_ENDPOINT defaults to the host as OrbStack names it.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
BB_TAG=20261020T112007Z-e88a90a
HOST=rill-exec-host
RILL_ENDPOINT=${RILL_ENDPOINT:-host.orb.internal:7470}
WORKERS=${RILL_WORKERS:-1}
CONCURRENCY=${RILL_WORKER_CONCURRENCY:-8}
inner() { docker exec -i "$HOST" "$@"; }

case "${1:-start}" in
stop)
  docker rm -f "$HOST" >/dev/null 2>&1 || true
  exit 0
  ;;
bazel)
  shift
  inner docker run --rm -i -v /src:/src -w "${RILL_SRC_DIR:-/src}" -v "${RILL_BAZEL_CACHE:-bazel-cache}:/root/.cache" \
    -v /config/bazelrc:/etc/bazel.bazelrc:ro rill-exec-runner bazel "$@"
  exit $?
  ;;
escape)
  inner rm -rf /src/escape
  docker cp "$here/escape" "$HOST:/src/escape"
  RILL_SRC_DIR=/src/escape "$0" bazel build //:report
  report=$(inner docker run --rm -v /src:/src -v "${RILL_BAZEL_CACHE:-bazel-cache}:/root/.cache" \
    rill-exec-runner sh -c 'cat /src/escape/bazel-bin/*.txt')
  echo "$report"
  if echo "$report" | grep -q ESCAPED; then exit 1; fi
  echo "$report" | grep -q 'kernel: .*gvisor' || { echo "not running under gVisor" >&2; exit 1; }
  exit 0
  ;;
sample)
  inner rm -rf /src/sample
  docker cp "$here/sample" "$HOST:/src/sample"
  RILL_SRC_DIR=/src/sample exec "$0" bazel run //:hello
  ;;
start) ;;
*) echo "usage: $0 [start|stop|bazel ARGS...]" >&2; exit 2 ;;
esac

: "${RILL_WORKER_SECRET:?set RILL_WORKER_SECRET to a secret the server lists}"
docker build -q -t rill-exec-gvisor-host "$here/gvisor-host" >/dev/null
docker rm -f "$HOST" >/dev/null 2>&1 || true
docker run -d --privileged --name "$HOST" -v "$here/config:/config:ro" \
  -v "$here/../helm/rill/files/exec:/buildbarn:ro" rill-exec-gvisor-host >/dev/null
until inner docker info >/dev/null 2>&1; do sleep 1; done
arch=$(inner dpkg --print-architecture)
inner docker build -q --build-arg TARGETARCH="$arch" -t rill-exec-runner - < "$here/runner/Dockerfile" >/dev/null
inner mkdir -p /bb /src
inner docker run --rm -v /bb:/bb "ghcr.io/buildbarn/bb-runner-installer:$BB_TAG" /bb >/dev/null 2>&1
i=1
while [ "$i" -le "$WORKERS" ]; do
  inner mkdir -p "/worker$i/build" "/worker$i/cache"
  inner docker run -d --name "runner$i" --runtime=runsc --network=none \
    --restart=always --pids-limit 32768 --memory 6g --read-only --tmpfs /tmp \
    -v /bb:/bb:ro -v /buildbarn:/buildbarn:ro -v "/worker$i:/worker" \
    rill-exec-runner sh -c 'rm -f /worker/runner; exec /bb/bb_runner /buildbarn/runner.jsonnet' >/dev/null
  inner docker run -d --name "worker$i" \
    -e RILL_ENDPOINT="$RILL_ENDPOINT" -e RILL_WORKER_SECRET="$RILL_WORKER_SECRET" \
    -e RILL_WORKER_NAME="worker$i" -e RILL_WORKER_CONCURRENCY="$CONCURRENCY" \
    -v /buildbarn:/buildbarn:ro -v "/worker$i:/worker" \
    "ghcr.io/buildbarn/bb-worker:$BB_TAG" /buildbarn/worker.jsonnet >/dev/null
  i=$((i + 1))
done
echo "$WORKERS worker(s) of $CONCURRENCY slots against $RILL_ENDPOINT"
