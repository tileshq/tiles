#!/usr/bin/env bash
# Read-only: is the release binary worth driving? Prints one line per check
# and exits non-zero on the first failure.
set -euo pipefail
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
bin="$root/target/release/tiles"
fail() { echo "doctor: FAIL $*"; exit 1; }
[ -x "$bin" ] || fail "no release binary at $bin; run: cargo build --release -p tiles-cli"
# What goes into the binary: sources, embedded assets and add-ons, and
# manifests (tests do not).
newest="$(find "$root"/crates/*/src "$root"/crates/*/assets "$root/adapters" "$root"/crates/*/Cargo.toml "$root/Cargo.toml" "$root/Cargo.lock" -type f ! -name '*.test.ts' -newer "$bin" -print -quit 2>/dev/null)"
[ -z "$newest" ] || fail "binary is older than $newest; rebuild it"
version="$("$bin" --version)"
case "$version" in "tiles "*"(intent schema 1)") echo "doctor: ok $version";; *) fail "unexpected version line: $version";; esac
forms="$("$bin" forms | grep -c '^[a-z]')"
[ "$forms" -ge 15 ] || fail "tiles forms lists $forms forms, want at least 15"
echo "doctor: ok $forms forms"
# The design the binary carries (ADR-037): design pack 1 for the house
# feature files, built from source for the plain theme.
design="$("$bin" --licence | sed -n 2p | cut -d: -f1)"
echo "doctor: ok carries $design"
command -v tmux >/dev/null || fail "tmux is not installed"
echo "doctor: ok tmux $(tmux -V | cut -d' ' -f2)"
echo "doctor: ok binary $(du -h "$bin" | cut -f1) at $bin"
