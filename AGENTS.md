# Project agent memory

Tiles draws gallery-quality chart cards inside the terminal for AI coding tools: a native Rust and Skia engine, an MCP server, and harness add-ons. Source-available under FSL-1.1-ALv2 (`LICENSE`); the house design pack is under `LICENSE-design`.

## Read first

- `docs/adr/README.md`: every decision, one record each. A change that contradicts a record adds a new record that supersedes it.
- `docs/definition-of-done.md`: every PR meets every point; each names its check.
- `docs/card-rules.md`: the card's craft rules the engine must match, each with the test that enforces it.
- `.agents/skills/verify-tiles/`: drive the real `tiles` CLI per chart form and keep evidence; run it before claiming a round done, and `maintain-verification-skill` on the features you touched.
- `docs/adr/ADR-010-engine-v0-1.md`: why text is drawn as glyph outlines and how close the engine must stay to the prototype.

## Build and check

- Run what CI runs: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `cargo deny check`, `cargo audit`, `python3 scripts/check-docs.py`, `python3 scripts/check-network-owners.py`, `python3 -m unittest scripts/test_policy_scripts.py`, `node --test adapters/pi/index.test.ts adapters/claude/card.test.ts`, `sh scripts/test_install.sh`, `sh scripts/test_package.sh`, `scripts/check-release-binary.py` on a release build made with `RUSTFLAGS="$(sh scripts/release-rustflags.sh)"`, gitleaks from `scripts/fetch-gitleaks.sh`, the fuzz targets (`fuzz/README.md`), and the formal proofs `sh verify/formal/painter/check.sh` and `sh verify/formal/kitty/check.sh` (ADR-026; skills `verify-painter-model` and `verify-kitty-encoder`). The jobs are in `.github/workflows/ci.yml`.
- Every byte Tiles writes to a terminal comes from `crates/tiles-term` (detection, the kitty and sixel encoder, the placeholder block, the sanitizer); a new output path joins the hostile-corpus tests over `fixtures/hostile/text.json`. Harness launch options live in `crates/tiles-cli/src/harness.rs`, per ADR-011.
- Before building on a harness's extension API, read the installed version's own docs or source (ADR-011 lists what was read); drive real harness sessions only from a scratch directory outside any repository, one at a time, never bypassing permissions or answering trust, login or import prompts.
- `TILES_BLESS=1 cargo test` rewrites the golden baselines (ADR-037). Only for an intended visual change, never to make a failing diff pass (ADR-006).
- A new chart form is a file in `crates/tiles-engine/assets/forms/` (added to `forms::EMBEDDED`), a fixture in `tests/intents/`, its goldens, a `<form>-worst` intent in `tests/intents/worst-case/` with its goldens (ADR-049) and a `verify-tiles` feature file. `TILES_DEV_ASSETS=<dir>` makes a debug build read tokens and forms from disk.
- A skia-safe feature change must match a prebuilt archive on rust-skia's release for macOS arm64, macOS x86_64 and Linux x86_64, or the build falls back to compiling Skia from source (ADR-002); a skia-safe version or feature change also updates the digests in `scripts/skia-binaries.sha256`, or the release build stops (ADR-014).
- Everything in a tool result reaches the model provider: chart results carry no picture, and a card drawn from Omni rows carries no values unless the user opted in (ADR-013).
- Work data, Omni credentials and real Omni responses never come to the development machine; Omni code is tested only against `fixtures/omni/` through the replay server in `crates/tiles-omni/src/replay.rs`, which refuses unrecorded calls (ADR-008).

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
