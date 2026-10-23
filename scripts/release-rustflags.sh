#!/bin/sh
# Prints the RUSTFLAGS a release build of `tiles` uses (ADR-022). rustc
# writes source paths into the binary for panic locations; these flags remap
# every one, so the binary names no builder's home directory, no checkout and
# no Tiles source file. Each workspace source file becomes `tiles/<n>`,
# numbered in sorted order, and dependencies' sources become `/deps/...`.
#
#   RUSTFLAGS="$(sh scripts/release-rustflags.sh)" cargo build --release -p tiles-cli
#   sh scripts/release-rustflags.sh --map    # tiles/<n> and the file it names
#
# Run --map at a release's tag to read a panic location from that release.
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
case "$root$cargo_home$HOME" in
  *[[:space:]]*) echo "release-rustflags: a path with a space cannot go in RUSTFLAGS" >&2; exit 1 ;;
esac
files="$(cd "$root" && find crates -name '*.rs' -not -path '*/target/*' | LC_ALL=C sort)"
if [ "${1:-}" = "--map" ]; then
  printf '%s\n' "$files" | awk '{ print "tiles/" NR, $0 }'
  exit 0
fi
# When several prefixes match, rustc applies the last one given, so the
# broadest come first.
flags="--remap-path-prefix=$HOME=~ --remap-path-prefix=$cargo_home=/cargo"
for index in "$cargo_home"/registry/src/*; do
  [ -d "$index" ] && flags="$flags --remap-path-prefix=$index=/deps"
done
flags="$flags --remap-path-prefix=$root=/tiles"
n=0
for file in $files; do
  n=$((n + 1))
  flags="$flags --remap-path-prefix=$file=tiles/$n --remap-path-prefix=$root/$file=tiles/$n"
done
printf '%s\n' "$flags"
