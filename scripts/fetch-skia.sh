#!/bin/sh
# Downloads the prebuilt Skia archive skia-bindings links into `tiles` for one
# target, checks it against the SHA-256 pinned in scripts/skia-binaries.sha256,
# and prints the SKIA_BINARIES_URL that makes the build read it from disk.
# skia-bindings itself fetches the archive unverified, so the release build
# uses only this copy (ADR-009). The URL names this exact file, not the
# `{key}` template: a feature change that is not pinned would otherwise miss
# the file and make skia-bindings build Skia from unverified source.
#
#   sh scripts/fetch-skia.sh <target> <dir>
#
# The pinned list must name the skia-bindings version in Cargo.lock; a new
# version needs its archives' digests added, from
# `gh api repos/rust-skia/skia-binaries/releases/tags/<version>`.
# TILES_SKIA_BASE and TILES_SKIA_SUMS override the download base and the
# pinned list, for scripts/test_policy_scripts.py.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
target="${1:?usage: fetch-skia.sh <target> <dir>}"
dir="${2:?usage: fetch-skia.sh <target> <dir>}"
sums="${TILES_SKIA_SUMS:-$here/skia-binaries.sha256}"
base="${TILES_SKIA_BASE:-https://github.com/rust-skia/skia-binaries/releases/download}"
version="$(awk '/^name = "skia-bindings"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' "$here/../Cargo.lock")"
[ -n "$version" ] || { echo "fetch-skia: no skia-bindings in Cargo.lock" >&2; exit 1; }
line="$(grep -E "^[0-9a-f]{64}  $version/skia-binaries-[0-9a-f]+-$target-[a-z-]+\.tar\.gz$" "$sums" || true)"
[ "$(printf '%s\n' "$line" | grep -c .)" = 1 ] || {
  echo "fetch-skia: $sums pins no single archive for skia-bindings $version on $target" >&2
  exit 1
}
want="${line%% *}"
path="${line##* }"
file="${path#*/}"
mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
curl -fsSL --retry 3 -o "$dir/$file" "$base/$path"
if command -v sha256sum >/dev/null 2>&1; then
  got="$(sha256sum "$dir/$file" | cut -d' ' -f1)"
else
  got="$(shasum -a 256 "$dir/$file" | cut -d' ' -f1)"
fi
if [ "$got" != "$want" ]; then
  rm -f "$dir/$file"
  echo "fetch-skia: $file has SHA-256 $got, not the pinned $want" >&2
  exit 1
fi
echo "file://$dir/$file"
