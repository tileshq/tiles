---
name: verify-kitty-encoder
description: Prove with Kani that the kitty graphics payload encoder (crates/tiles-term/src/b64.rs) turns any bytes into exactly their padded base64, so a chart's bytes can never put ESC or a backslash inside a kitty graphics command. Use after any change to b64.rs, when writing a Kani harness for kitty.rs's framing (the follow-ups in verify/formal/kitty/README.md), or when a review asks whether chart data can escape a graphics command.
---

# Verify the kitty encoder

Generated with `create-formal-verification-skill`. The proofs live in `verify/formal/kitty/`; its `README.md` states the property, the harnesses with their measured times, the bounds, the harnesses that did not finish, and what is left out. Read it first.

## Does the change touch the verified code?

Yes when the diff touches `crates/tiles-term/src/b64.rs`. A change to `crates/tiles-term/src/kitty.rs` is not proven yet: its framing harnesses are follow-ups, so run `cargo test -p tiles-term --locked` (the kitty unit tests and `tests/hostile.rs`) instead, and say in the PR that the framing has no proof. The painter (`crates/tiles-cli/src/wrap/paint.rs`) only calls the encoder; a painter change needs `verify-painter-model`. A dependency change in `tiles-term` or `tiles-intent` regenerates `verify/formal/kitty/Cargo.lock` (`cargo update --workspace` in `verify/formal/kitty`) and commits it, or the check stops at `--locked`.

## Run

```sh
sh verify/formal/kitty/check.sh
```

It needs Kani 0.68.0: `cargo install --locked kani-verifier --version 0.68.0`, then `cargo kani setup` (about 360 MB into `~/.kani`). Never run `cargo kani` on `tiles-engine`, `tiles-mcp` or `tiles-cli`: they link skia-safe, which Kani cannot build. `verify/formal/kitty` is its own workspace that depends on `tiles-term` only, so a workspace-wide build never compiles it. The script runs `cargo test --locked` there first, then each harness on its own with the time limit it lists (twice the time measured on the development Mac); run one Kani command at a time on that shared Mac.

A pass prints `VERIFICATION:- SUCCESSFUL` for each harness and the script exits 0. The unit tests are the checker's own; until a framing harness finishes, `the_checker_accepts_the_encoder_and_rejects_a_leak` is the check of the framing property over the real encoder, and it also rejects an appended OSC 52 leak.

## A failure

Kani prints the failed `assert!`; rerun that harness with `--concrete-playback=print` for a unit test that reproduces it.

- A real bug: `encode` wrote a byte outside the alphabet, the wrong padding, or text that does not decode to the input. Report it with the input Kani found; fix the encoder, never the checker.
- A modeling error: `encodes` in `verify/formal/kitty/src/lib.rs` reads base64 differently from RFC 4648 section 4. Fix the checker only after its unit tests show it refuses a string RFC 4648 accepts, or accepts one it refuses.
- A timeout: the machine was busy, or a change made the proof harder. Rerun that harness alone from `verify/formal/kitty` with a larger limit, `cargo kani --output-format terse -Z unstable-options --exact --harness proofs::<name> --harness-timeout <n>s`; never drop a harness to make the check pass. Record a new measured time in the README and set the limit in `check.sh` to twice it.

## An intended change

A change to `b64.rs` that keeps the output must keep every harness passing. Before trusting a new harness, break the code on a scratch copy (for example push `((n >> shift) & 63) as u8` instead of the alphabet character, or drop the `=` padding) and see the check fail; then drop the scratch change. A framing harness for `kitty.rs` uses `carries` and `bare` from `src/lib.rs`, and passes only once it finishes within its limit; until then it stays out of `check.sh` and is listed in the README.
