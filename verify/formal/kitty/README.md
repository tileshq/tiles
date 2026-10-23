# Kitty encoder proofs (Kani)

Rerun: `sh verify/formal/kitty/check.sh` (Kani 0.68.0). It runs `cargo test --locked` here first, the checker's own unit tests, then the three harnesses one at a time, each within the limit in the table below.

## Property

Nothing from a chart reaches the terminal through the kitty encoder except base64 inside the encoder's own framing. If a chart's title or values ever ended up in a PNG's bytes, a payload holding `ESC \` could end the graphics command early and the bytes after it would run as terminal commands (OSC 52 writes the clipboard, OSC 8 forges links). ADR-009 rests on this.

The proven part is the payload: `tiles_term::b64::encode` (`crates/tiles-term/src/b64.rs`) of any byte string is exactly its standard padded base64 (RFC 4648 section 4), so it holds only the 64 alphabet characters and `=`, never `ESC` or `\`, and decodes back to the input. `src/lib.rs`'s `encodes` states that independently of the encoder: it walks the input's 3-byte groups and checks each 4-character group's sextets, padding and leftover bits.

| Harness | Inputs | Measured | Limit in check.sh |
|---|---|---|---|
| `b64_round_trips_1_byte` | every 1-byte string (two `=`) | 89 s | 180 s |
| `b64_round_trips_2_bytes` | every 2-byte string (one `=`) | 49 s | 100 s |
| `b64_round_trips_3_bytes` | every 3-byte string (no padding) | 109 s | 220 s |

Each limit is twice the time measured on the development Mac, an 8 GB dual-core 2017 MacBook shared with another Rust build (load average 5 to 16 while these ran).

A scratch break of the real `b64.rs` (pushing the sextet's value instead of `ALPHABET[sextet]`) made `b64_round_trips_1_byte` fail in 42 s with `assertion failed: encodes(coded.as_bytes(), &bytes)`; the break was dropped.

### A refactor for verifiability

`b64::encode` used to push each character with `String::push(char::from(..))`. Kani cannot tell that such a char is one byte, so every later length was symbolic: a 3-byte input spent 238 s in symbolic execution and never reached the solver. It now pushes bytes into a `Vec<u8>` and converts once with `String::from_utf8`, which cannot fail because every byte is ASCII. The output is identical; the RFC 4648 test vectors and the every-byte test in `b64.rs` are unchanged and pass. After the change, encoding alone of every 3-byte string proved in 13 s.

## Bounds and assumptions

- Inputs of 1, 2 and 3 bytes. `encode` handles each 3-byte group alone (`bytes.chunks(3)`), so these three cover every group shape: two `=`, one `=`, none. A longer input repeats the same loop body once per group; that it does so correctly for a group after a group is an argument, not a proof, because the 4-byte harness did not finish (below). The empty input is the RFC 4648 test vector `("", "")`.
- Kani's model of `Vec` and `String::from_utf8` is trusted.

## Not proven here (follow-ups)

These harnesses were written and did not finish within a 3-minute limit on the development Mac. A timeout on that machine is not a verdict on the proof; each is worth rerunning on a faster one (`--harness-timeout` larger).

| Harness | Code | Bound | Result |
|---|---|---|---|
| `b64_round_trips_4_bytes` | `b64::encode` | every 4-byte string | over 180 s |
| `b64_round_trips_1_to_6_bytes` (part 1) | `b64::encode` | every string of 1 to 6 bytes, one harness | over 5 min, with a decoding checker since replaced by `encodes` |
| `payload_commands_carry_only_base64_empty` | `show_png`, `virtual_png`, `transmit_png`, `frame_png` | an empty PNG; any id, size, crop and gap | over 180 s |
| `payload_commands_carry_only_base64_4_bytes` | the same | every 4-byte PNG; any id, size, crop and gap | not run after the empty case timed out |
| `commands_without_a_payload_are_one_frame_of_keys` | `virtual_place`, `place`, `delete_placement`, `show_frame`, `delete` | any id, placement, size, z, crop and frame | not run; `delete(any id)` alone took over 180 s |
| `a_second_chunk_carries_only_base64` (part 1) | `transmit_png`, the chunk loop in `command` | a 3073-byte PNG, seven free bytes, unwind 4200 | dropped; the unit test `a_large_png_is_chunked_with_keys_only_on_the_first` in `kitty.rs` covers the split |

The framing harnesses stall in `core::fmt`: every public function in `kitty.rs` formats its keys with `format!`, and `delete(u32::MAX)` with a fixed id still did not finish in 180 s. A probe that called the private `command(keys, payload)` through a `#[cfg(kani)]` entry with fixed keys and an empty payload did not finish either, under a load average of 5 to 11, so the cost is not in `format!` alone. The checker those harnesses need is in `src/lib.rs` (`frames`, `carries`, `bare`). Until a framing harness finishes, the framing property is checked by its unit test `the_checker_accepts_the_encoder_and_rejects_a_leak`, which `check.sh` runs over the real encoder's output on every CI run and which also rejects an appended OSC 52 leak; the unit tests in `crates/tiles-term/src/kitty.rs`, the hostile-corpus tests in `crates/tiles-term/tests/hostile.rs` and the fuzz targets (ADR-024) cover the framing beside it.

## Left out

- `kitty::placeholder_rows` (text, not a graphics command; its colour SGR carries only the id's bytes) and `QUERY` (a constant).
- Sixel (`crates/tiles-term/src/sixel.rs`) and the text sanitizer; the hostile-corpus tests and the fuzz targets cover them.
- Whether a terminal accepts the keys; the property is about what can reach it.

## When to look again

Any change to `crates/tiles-term/src/b64.rs`: rerun the check. A change to `crates/tiles-term/src/kitty.rs` is not covered by these proofs yet; run its unit tests and the hostile-corpus tests. A dependency change in `tiles-term` or `tiles-intent` regenerates `verify/formal/kitty/Cargo.lock` (`cargo update --workspace` in `verify/formal/kitty`) and commits it, or the check stops at `--locked`.
