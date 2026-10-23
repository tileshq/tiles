#!/bin/sh
# Runs this crate's unit tests, then proves tiles_term::b64::encode with Kani
# (src/proofs.rs), one harness at a time, each stopped at twice the time it
# took on the development Mac (README.md). The unit tests run the framing
# checker over the real encoder, the one check of the framing until a Kani
# harness for it finishes. Exits non-zero when a test or proof fails or a
# proof times out; Kani prints the failed check.
# Needs Kani 0.68.0 (`cargo install --locked kani-verifier --version 0.68.0`
# then `cargo kani setup`), and refuses to run before that setup is done, so
# the cargo-kani proxy never fetches a bundle of its own, unchecked.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
kani_home="${KANI_HOME:-$HOME/.kani}"
[ -x "$kani_home/kani-0.68.0/bin/kani-driver" ] || { echo "check.sh: needs Kani 0.68.0 set up (cargo kani setup)" >&2; exit 1; }
version="$(cargo kani --version 2>&1 || true)"
case "$version" in
  *"Kani Rust Verifier 0.68.0"*) ;;
  *) echo "check.sh: needs Kani 0.68.0, found: $version" >&2; exit 1 ;;
esac
echo "== unit tests"
cargo test --locked
for proof in b64_round_trips_1_byte:180s b64_round_trips_2_bytes:100s b64_round_trips_3_bytes:220s; do
  echo "== ${proof%%:*} (limit ${proof#*:})"
  cargo kani --output-format terse -Z unstable-options \
    --harness-timeout "${proof#*:}" --exact --harness "proofs::${proof%%:*}"
done
