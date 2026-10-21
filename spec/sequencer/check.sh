#!/bin/sh
# Model check the sequencer: TLC from tla2tools.jar (TLA_JAR, default
# ~/.local/lib/tla2tools.jar) on Java 11 or newer (JAVA, default `java`).
set -eu
cd "$(dirname "$0")"
exec "${JAVA:-java}" -XX:+UseParallelGC -cp "${TLA_JAR:-$HOME/.local/lib/tla2tools.jar}" \
    tlc2.TLC -workers auto -deadlock -config MCSequencer.cfg MCSequencer.tla
