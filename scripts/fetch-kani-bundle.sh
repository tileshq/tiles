#!/bin/sh
# Downloads the Kani release bundle for Linux x86_64 (its compiler, CBMC and
# solvers, about 140 MB) into <dir>, checks it against the SHA-256 pinned
# below, and prints its path. CI's `formal` job hands it to Kani's setup
# (ADR-026), so the proofs never run on a bundle fetched unchecked:
#
#   cargo kani setup --use-local-bundle "$(sh scripts/fetch-kani-bundle.sh /tmp/kani)"
#
# The release publishes no checksum file, so a new version's digest is taken
# from a download of the bundle and checked against a second download.
# TILES_KANI_BUNDLE_BASE overrides the download base, for
# scripts/test_policy_scripts.py.
set -eu
dir="${1:?usage: fetch-kani-bundle.sh <dir>}"
version=0.68.0
want=32e2b484d73ede0bbf64a2cf0879c4259422497d8aec4ee67d448de9ae7843d3
base="${TILES_KANI_BUNDLE_BASE:-https://github.com/model-checking/kani/releases/download}"
file="kani-$version-x86_64-unknown-linux-gnu.tar.gz"
mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
bundle="$dir/$file"
sum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}
if [ ! -f "$bundle" ] || [ "$(sum "$bundle")" != "$want" ]; then
  curl -fsSL --retry 3 -o "$bundle" "$base/kani-$version/$file"
fi
got="$(sum "$bundle")"
if [ "$got" != "$want" ]; then
  rm -f "$bundle"
  echo "fetch-kani-bundle: $file has SHA-256 $got, not the pinned $want" >&2
  exit 1
fi
echo "$bundle"
