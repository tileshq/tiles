# ADR-006: Quality is proved by image diffs against golden renders, scene-level rule tests, a driven verification skill and, later, a chart-quality eval set

- Status: Accepted
- Date: 2026-10-17

## Context

The owner asked for v0.1 "fully tested to the top standard". The look he approved is the prototype's HTML card; the engine has to reproduce it without a browser (ADR-002). A card can be wrong in three ways: it looks different from the approved card, it breaks a craft rule (an accent on the wrong bar), or it shows in the terminal wrongly. Looking at screenshots catches none of these reliably, and agents asked to fix a failing visual test are tempted to edit the baseline.

## Decision

### 1. Golden renders first, asserted by image diff

- **Reference images** come from the approved look: a developer-only tool renders the prototype's HTML card in headless Chrome for a fixed set of intents and commits the PNGs. They are regenerated only by that tool from the prototype, never by the engine.
- **Engine baselines** are the engine's own committed output for the same intents, compared on every CI run on macOS and Linux with a per-channel tolerance that absorbs only floating-point and SIMD differences between machines.
- The engine round measures how close the engine gets to each reference and writes the threshold into the test. Thresholds can be tightened in any PR; loosening one needs an ADR.
- **No diff is ever passed by eye, and a baseline is never edited to make a diff pass.** Regenerating engine baselines (`TILES_BLESS=1 cargo test`) is allowed only for an intended visual change, and the PR shows the before, after and diff images and says why the pixels changed.
- A failing comparison writes the actual image and a diff image under `target/tmp/golden-diff/`, so the failure is visible without re-running.
- The harness is `crates/tiles-engine/tests/golden.rs`. It starts with one trivial case; round 2 adds a case per form and theme.

### 2. Craft rules are tested on the scene, not the pixels

Each rule in `docs/playbook.md` that a machine can check becomes a unit test over the engine's scene: the accent colour appears only on marks the highlight names, bars carry their values, bar charts have no y axis, bars start at zero. A rule test fails with the rule's id. Golden images catch drift; rule tests say which rule broke.

### 3. A project-local verification skill drives the real thing

Tiles uses pstack's two verification skills, vendored unmodified under `.agents/skills/` with their MIT licence and attribution (`.agents/skills/README.md`). No Cursor plugin is needed to run them.

- **The engine round creates `verify-tiles`** with `create-verification-skill` as soon as the CLI renders a PNG, at `.agents/skills/verify-tiles/`:
  - **Launch:** build the release binary once; each drive runs in its own tmux session or PTY, the control-cli pattern from Cursor's team kit.
  - **Doctor:** a read-only check that the binary is the right build and the harness under test is installed.
  - **Drive:** a feature map with one file per chart form and one per surface (CLI render to PNG, the MCP tool over stdio, then Pi, OpenCode, Claude Code and Codex as their rounds land). Each drives one chart from a fixed intent the way a user would.
  - **Evidence:** the PNG, the terminal transcript and the graphics escapes written, kept under `target/verify/<run>/`.
  - **Cleanup:** kill only the sessions the run started, and keep the evidence.
- **Every later round runs `maintain-verification-skill`** on the features it touched, and the PR states the outcome (clean, changed or blocked).

A PTY or tmux session stands in for Warp and Terminal.app, so it proves the bytes Tiles writes, not each terminal's pixels. The skill lists what the stand-in does not verify, and those items stay open until a run in that terminal covers them.

### 4. Later: the chart-quality eval set

A few hundred real questions with the expected form, highlight and title tone, seeded from the prototype's 53-form library. Each run scores a base model's own chart choice against Tiles' intent on the same question. It is the measure the moat claims (ADR-005) rest on, and it lands with the hosted intelligence work.

## Consequences

- Golden tests need deterministic rendering, which ADR-002 provides with bundled fonts and CPU raster.
- Baseline PNGs live in the repository; they are small at card size.
- The definition of done gains a golden-test point now and a verification-skill point when the skill lands.

Sources: pstack `create-verification-skill`, `maintain-verification-skill` and the visual-parity playbook, https://github.com/cursor/plugins/tree/main/pstack (commit `0317eb8`, fetched 2026-10-17).
