#!/bin/sh
# Model-checks the wrapper painter's kitty motion route (Painter.tla) with
# TLC: the safety properties in Painter.cfg over two cards with two-row
# blocks, then the liveness property in Liveness.cfg over one card. Exits
# non-zero on a counterexample, which TLC prints as a numbered trace of
# states. Needs Java 11 or newer; fetches the pinned tla2tools.jar into
# ~/.cache/tiles-tla.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
jar="$(sh "$root/scripts/fetch-tla2tools.sh" "$HOME/.cache/tiles-tla")"
states="$(mktemp -d)"
trap 'rm -rf "${states:?}"' EXIT
cd "$here"
for cfg in Painter.cfg Liveness.cfg; do
  echo "== $cfg"
  java -XX:+UseParallelGC -cp "$jar" tlc2.TLC -workers auto -cleanup \
    -metadir "$states/${cfg%.cfg}" -config "$cfg" Painter.tla
done
