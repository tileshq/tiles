---
name: verify-tiles
description: Drive the real Tiles CLI the way a user does (tiles render an intent to a PNG, tiles forms) in an isolated tmux session and keep the PNG, frames, transcript and exit code as evidence. Use to prove a chart form, a theme, the entrance frames or a CLI error path works after any engine, form or CLI change, and before claiming a round is done.
---

# Verify Tiles

Tiles draws chart cards from a chart intent (`docs/adr/ADR-003-chart-intent.md`). The user-facing surfaces are the `tiles` CLI, the MCP server (`tiles mcp`), `tiles run` with its terminal routes, and `tiles install`, `tiles doctor` and `tiles check`; each has a feature file. Each chart form has a feature file in `features/`, and every fixture intent lives in `crates/tiles-engine/tests/intents/<form>.json`.

## Launch

`tiles` is a short-lived CLI, so there is no server to keep alive. Build the release binary once per run from the repository root:

```sh
cargo build --release -p tiles-cli
```

The form, render and theme feature files describe the official release, which carries the sealed house design from design pack 1 (Obsidian the default, then light and dark); the pack ships only in official releases. A build from source carries the plain theme, dark (the default) and light, and [the plain theme](features/plain-theme.md) is its recipe (ADR-037). `scripts/doctor.sh` prints which design the binary carries.

Ready means `scripts/doctor.sh` passes. Every drive then runs in its own tmux session named `tiles-verify-<run>-<name>`, started and killed by `scripts/drive.sh`. Pick a run id that is unique to this verification run, such as `$(date +%H%M%S)`.

## Doctor

```sh
.agents/skills/verify-tiles/scripts/doctor.sh
```

Read-only. It fails when the release binary is missing or older than any crate source, embedded asset, add-on under `adapters/` or manifest, when `tiles --version` does not name intent schema 1, when `tiles forms` lists fewer than 15 forms, or when tmux is missing. Run it first, and again whenever a drive surprises you.

## Drive

```sh
.agents/skills/verify-tiles/scripts/drive.sh <run> <name> <tiles args...>
```

`$OUT` inside the arguments (single-quoted, so the shell leaves it alone) becomes the evidence directory. `--env NAME=value` before the tiles arguments sets a variable for that command only. Example, the column form in the dark theme with its entrance frames:

```sh
.agents/skills/verify-tiles/scripts/drive.sh r1 column-dark render crates/tiles-engine/tests/intents/column.json -o '$OUT/card.png' --theme dark --frames '$OUT/frames' --timings
```

The script types the command into the tmux session, waits for it to finish, captures the pane and kills the session. The feature files list the exact commands and the end state that proves each feature.

## Evidence

Everything lands in `target/verify/<run>/<name>/`:

- `command.txt`: the exact command typed.
- `transcript.txt`: the terminal as the user saw it, stdout and stderr.
- `exit-code.txt`: `exit=<n>`.
- `card.png`, and `frames/frame-00.png` to `frame-21.png` when frames were asked for.
- `files.txt`: a short hash, type and pixel size of each PNG.

Proof standards: drive the real binary through a terminal, never call engine functions directly. Read the PNG yourself (open it as an image) and check the end state the feature file names: which mark carries the accent, which values are written, the theme. A zero exit code alone proves nothing about the picture. For an error path, the proof is the one-line message in the transcript and a non-zero exit code, and no output file.

## Cleanup

`drive.sh` kills its own session on exit, success or failure. To clear a stranded session from an interrupted run, kill it by its exact name: `tmux kill-session -t tiles-verify-<run>-<name>`. Never kill tmux sessions by pattern or tmux itself, and never delete `target/verify/`: it holds the proof.

## Other drivers

- `scripts/drive-mcp.sh <run> <name> <intent.json> [VAR=value ...]` feeds a harness's first three requests to `tiles mcp` and keeps `requests.jsonl`, `replies.jsonl`, `summary.txt`, `stderr.txt` and the cards.
- `scripts/drive-kitty.sh <run> <name> <kitty|warp>` (macOS, `TILES_KITTY` set to a kitty binary) opens a kitty window running `tiles run` over a fake harness that prints a block, and keeps `window.png`. It closes only the window it opened.
- `cargo test -p tiles-cli --test pty` drives every route byte for byte through a pseudo-terminal.

## What a tmux session does not prove

A tmux pane stands in for Warp, Terminal.app, herdr and the harnesses. It proves the bytes Tiles writes and the files it makes, not how any terminal shows them; under tmux the route is text by design. Pixels in a terminal are proved in kitty with `drive-kitty.sh`; Warp is checked by hand on a laptop, in `docs/laptop-check.md`.
