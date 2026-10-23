#!/bin/sh
# Downloads the TLA+ tools (TLC) into <dir>, checks the jar against the
# SHA-256 pinned below, and prints the jar's path. The formal models under
# verify/formal/ run with it (ADR-026); it needs a Java 11 or newer runtime:
#
#   jar="$(sh scripts/fetch-tla2tools.sh /tmp/tla)"
#   sh verify/formal/painter/check.sh
#
# The release publishes no digest, so a new version's is taken from a
# download of the jar and checked against a second download.
# TILES_TLA2TOOLS_BASE overrides the download base, for testing.
set -eu
dir="${1:?usage: fetch-tla2tools.sh <dir>}"
version=1.7.4
want=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
base="${TILES_TLA2TOOLS_BASE:-https://github.com/tlaplus/tlaplus/releases/download}"
mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
jar="$dir/tla2tools-$version.jar"
sum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}
if [ ! -f "$jar" ] || [ "$(sum "$jar")" != "$want" ]; then
  curl -fsSL --retry 3 -o "$jar" "$base/v$version/tla2tools.jar"
fi
got="$(sum "$jar")"
if [ "$got" != "$want" ]; then
  rm -f "$jar"
  echo "fetch-tla2tools: tla2tools.jar has SHA-256 $got, not the pinned $want" >&2
  exit 1
fi
echo "$jar"
