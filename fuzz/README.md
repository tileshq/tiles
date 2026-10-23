# Fuzz targets

One libFuzzer target per parser of untrusted input ([ADR-024](../docs/adr/ADR-024-security-baseline-in-ci.md)): `terminal_stream` (the wrapper's painter, compiled from `crates/tiles-cli/src/wrap/`), `terminal_replies`, `intent`, `block` and `warehouse_source` (a warehouse source's SQL with its `{{ ref(...) }}` model references, through `crates/tiles-intent`'s reader and renderer; its first byte picks the target). CI's `fuzz` job runs each target in `fuzz_targets/` for 60 seconds from the seed corpus in `corpus/<target>/`.

To run one locally, from the repository root, on a copy of its corpus so the committed seeds stay small:

```sh
rustup toolchain install nightly-2026-10-01 --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
cp -R fuzz/corpus/intent /tmp/intent-corpus
cargo +nightly-2026-10-01 fuzz run intent /tmp/intent-corpus -- -max_total_time=60
```

A crash leaves its input in `fuzz/artifacts/<target>/`. Fix the bug, then add that input to `corpus/<target>/` under a name that says what it holds, so CI replays it on every run.

`terminal_stream` runs far fewer inputs a second than the others: an input that narrows the screen past a wide character makes `vt100` panic, the painter contains it (`crates/tiles-cli/src/wrap/paint.rs`), and unwinding costs about 10 ms. Those inputs are the point of the target, so they stay.
