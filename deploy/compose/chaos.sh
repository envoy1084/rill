#!/usr/bin/env bash
# Kill rill servers at random while load runs: SIGKILL, no drain, no
# lease release, then start them again. Run alongside rill-load, whose
# ledger then checks that no acknowledged work was lost.
#
#   ./chaos.sh 600            # ten minutes of kills
#   CHAOS_TARGETS="rill1 rill2 rill3 lb" ./chaos.sh 600
set -euo pipefail
cd "$(dirname "$0")"

duration=${1:-600}
targets=(${CHAOS_TARGETS:-rill1 rill2 rill3})
min_gap=${CHAOS_MIN_GAP:-10}
max_gap=${CHAOS_MAX_GAP:-40}
down=${CHAOS_DOWN:-5}

end=$((SECONDS + duration))
kills=0
while [ "$SECONDS" -lt "$end" ]; do
  sleep $((min_gap + RANDOM % (max_gap - min_gap + 1)))
  victim=${targets[$((RANDOM % ${#targets[@]}))]}
  echo "$(date -u +%H:%M:%S) kill $victim"
  docker compose kill -s KILL "$victim" >/dev/null
  sleep "$down"
  docker compose start "$victim" >/dev/null
  kills=$((kills + 1))
done
echo "chaos: $kills kills in ${duration}s"
