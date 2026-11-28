# ADR-063: The interactive dashboard pane is built in Tiles' own Skia engine, in the terminal first, behind a one-week prototype that decides go or no-go

- Status: Accepted; the Ghostty run passed every timed gate, and [ADR-065](ADR-065-pane-ships-with-fetched-rows-and-one-interaction-state.md) ships the pane in every build with fetched rows and one interaction state, amending the prototype's absence from release builds and its refusal of data sources; [ADR-070](ADR-070-pane-polish-and-focus-on-click.md) leaves the pane out of release builds again, through `debug_assertions`, until a later record turns it on at launch
- Date: 2026-11-26

## Context

On 26 November 2026 the owner agreed to five recommendations from two research reports on the engine's direction. In the owner's words: "my goal is to that yes we render charts in terminal, that's fine as v0, we do final testing today, and then share on monday, while at the same time, we start the side pane/browser thingy? so then that chart goes into a pane/link type thing? idk, and that's v1". The owner's goal for the work, the same day: "super fast, super smooth, super lightweight, highly performant, and battle tested against all security vulnerabilities."

The five recommendations:

1. Keep Skia, and build the interactive dashboard pane inside Tiles' own engine, with a one-week prototype as the go or no-go gate. Pixelgui (`zenbu-labs/terminal-browser`), the library that prompted the question, rasterises with tiny-skia, a Skia subset its own README calls 20 to 100% slower on x86-64 and 100 to 300% slower on ARM, and its text stack has no shaping. What it has and Tiles lacks is input, redraw and pane transport, which Tiles can write on the scene and hover hit map it already keeps (ADR-002).
2. The terminal pane comes before any browser dashboard for teammates, keeping [ADR-001](ADR-001-product-scope.md)'s order: full-page dashboards in the terminal at stage 3, a link or window at stage 4.
3. The prototype is judged in Ghostty. Warp and Orca's terminal are checked by hand, and each result decides whether that terminal's users get the pane or the companion window.
4. Orca becomes a named Tiles terminal only after the go: a `TERM_PROGRAM=Orca` branch in detection and Orca among the pane's split openers.
5. Tiles serves no web page to Orca's browser pane before the browser-link stage: a page would need a second renderer, a listening port or `file:` pages ([ADR-009](ADR-009-security-baseline.md) says Tiles listens on no port), and a page any agent in the Orca workspace can read, which breaks [ADR-013](ADR-013-data-stays-local.md)'s opt-in for values.

ADR-001 lists interactivity as a v0.1 non-goal and says each stage gets its own ADR before work starts. This is that record for stage 3's interactive pane.

## Decision

### The direction

The interactive dashboard is a pane in the terminal, drawn by the Skia engine that draws inline cards. After a go, the work runs in this order, each step its own pull request and, where it changes a decision, its own ADR:

| Order | Work |
|---|---|
| 1 | The pane, shippable: interaction state in the scene; cross-filter, legend toggle, sort and drill; `tiles_pane_open` and `tiles_pane_state` MCP tools that carry no values unless the user opted in (ADR-013); split openers for kitty, Ghostty, tmux, WezTerm, iTerm2, herdr, Rex, VS Code and Orca |
| 2 | Orca named as a terminal, on the pre-cut route, proved in a real Orca session as ADR-062 proved Rex |
| 3 | The companion window ([ADR-046](ADR-046-companion-window-on-the-text-route.md)) draws scenes itself and takes the mouse, for Warp, Terminal.app and tmux |
| 4 | A table form with virtual rows and cached text |
| 5 | The browser link, ADR-001 stage 4, with its own ADR; Orca's browser split becomes one more place that opens it |

Inline cards, the placeholder block, the wrapper and every golden stay as they are through all five.

### The week-one prototype

`tiles pane <fixture> [--trace <file>]` is compiled only with debug assertions on: a debug build, or the `pane` profile (`cargo build --profile pane -p tiles-cli`), an optimized build with debug assertions that exists to measure it. A release build has no `pane` command. The usage text leaves it out, and no MCP tool, add-on or `tiles run` path reaches it. A fixture is a chart call as the `tiles_chart` tool takes it, read by `tiles_mcp::card::Call`, which refuses a call naming Omni or a warehouse, so the pane reaches no data source; or a table stress scene (`"stress": "table"`).

- **The screen.** It takes the terminal in raw mode and the alternate screen, probes mode 1016 (pixel mouse reports, by DECRQM) and the cell size (`CSI 16 t`), ending the probe at the device attributes reply or after 500 ms, then turns on modes 1003 and 1006, and 1016 when the terminal knows it. Without 1016 a report's cell is read as its middle pixel, cell position times cell size.
- **Cards.** The dashboard planner (`tiles_mcp::board::plan`) lays the cards out at the pane's width, exactly as the chart tool would. Each card is its own kitty image, sent as raw RGBA compressed with zlib (`f=32,o=z`, `kitty::show_rgba_zlib`) at its own pixel size, so the terminal never scales it. Each card swaps between two image ids, and every update is wrapped in synchronized output (mode 2026), so a replaced card never shows a gap.
- **Hover.** The pointer's pixel finds its card and the smallest hit-map target under it (`hit::hit_at`). The mark's outline and its tooltip are drawn as one small picture (`Engine::hover`), placed above the card at its pixel offset (`X`, `Y`, `z=2`). A card is never sent again on hover; moving off every mark deletes the overlay. Motion reports read together coalesce to the last.
- **Click.** A left click on a mark selects the label its row holds in the first of its `series` and `x` columns that another card's table also holds, or, when no other card holds either, in the first its own form highlights by. Every other card holding that column keeps only the rows with that label, filtered in memory; the clicked card highlights it when its form highlights by that column (`Form::highlight_columns`, the engine's own rule), so a stacked bar clicked by a shared `x` draws without a highlight rather than refusing. Only cards whose scene changed are laid out again, in their own slot, and sent; in the same update the overlay is drawn again for the mark under the pointer in the new scene, or deleted when no mark is there. Clicking the selection again, or empty space, clears it. A card that refuses its filtered rows keeps its picture and is named on the status line.
- **Resize.** SIGWINCH, or a new cell size, plans and draws everything again from a cleared screen.
- **Table stress scene.** 40 rows by 8 columns of text in one card (`Engine::text_grid`), scrolled a row a step by the wheel or the arrows, a page by Page Up and Page Down, with the steps read together drawn once. It prices a table before a table form exists.
- **The way out.** `q`, Control-C or Control-D, SIGINT, SIGTERM, SIGHUP or SIGQUIT (routed to a self-pipe through `signal-hook`), the end of input, an error, and a panic (from the panic hook, before its message prints) all give the screen back once: every mode off, every picture deleted, the cursor shown, the alternate screen left and the terminal's settings restored. Only SIGKILL cannot.
- **Idle.** With no input the loop blocks in `poll` with no timeout, so an idle pane wakes for nothing. `--trace` adds one sampler thread that wakes once a second.
- **Shared memory.** The shared-memory transport (`t=s`) is not built. Inline zlib met every number in the pseudo-terminal run below; it is built only if Ghostty misses the hover gate.

### Security

Every byte the pane writes comes from `tiles-term`: `tiles_term::pane` (modes, cursor moves, the status line through the one sanitizer) and `tiles_term::kitty`. The status line joins the hostile-corpus test (`crates/tiles-term/tests/hostile.rs`). Everything the terminal sends goes through `tiles_term::input::Parser`, which is total: any byte sequence parses or is dropped, a sequence keeps at most 32 parameter bytes, a string sequence is abandoned after 64 KiB, Control-C ends any sequence, and the events do not depend on how the bytes were split between reads. The `pane_input` fuzz target holds it to that from its first commit and joins CI's `fuzz` job, which runs every target in `fuzz/fuzz_targets/` for 60 seconds under AddressSanitizer ([ADR-024](ADR-024-security-baseline-in-ci.md)); a property test over random input does the same in `cargo test`. The pane opens no port, writes no web page and reaches no network. Text on a card reaches the terminal as pixels only (ADR-009).

Two dependencies join `tiles-cli`: `flate2` 1.1.10 with only its pure-Rust backend (MIT or Apache-2.0, already in the lock file as a build dependency of Skia's), for zlib; and `signal-hook` 0.4.6 (MIT or Apache-2.0), for the self-pipe. Neither reaches the network. No code was copied from pixelgui.

### The measurements that decide go or no-go

`--trace` writes a line at each mouse report read, each card or overlay drawn and each update written, and once a second the resident memory and CPU time. `scripts/pane-trace.py <trace> --machine work|intel` turns a trace into this table; the thresholds are the research reports':

| Measure | Go, work laptop | Go, 2017 Intel Mac |
|---|---|---|
| Hover, mouse report read to bytes written, p95 | 16 ms | 33 ms |
| Hover as seen in a 60 fps screen recording | Tooltip within 2 frames of the pointer, no flicker | Same |
| Bytes per hover update | 64 KB or less | 64 KB or less |
| One card redrawn after a click | 16 ms | 50 ms |
| Eight-card dashboard redrawn at a half-screen Retina pane | 50 ms | 150 ms |
| Table stress scene (40 rows by 8 columns), one scroll step | 33 ms | Recorded |
| Start to first full frame | 200 ms | 200 ms |
| Resident memory, steady | 100 MB | 100 MB |
| Idle CPU with the pointer still | Under 1% | Under 1% |
| Fuzz target on the input parser | 60 s under AddressSanitizer, no crash | Same |
| Warp, Orca's terminal | Recorded; each decides pane or window for that terminal | Same |

A miss on the table scroll step means cached row bitmaps, a known fix, and does not stop the pane.

No-go rule: if hover p95 misses by more than double after the shared-memory step, or Ghostty flickers in a way synchronized output does not fix, stop and try pixelgui's `pixel-kitty` crate, vendored with Zenbu Labs' MIT notice, as the transport under the same scene. If that also misses, the pane moves to a native window drawing the same scenes and the terminal keeps inline cards. No branch of this rule leads to a browser.

## Consequences

- ADR-001's interactivity non-goal is amended: the pane exists as a prototype outside release builds, and ships only after a go and the pane's own pull request.
- Measured in a pseudo-terminal on the 2017 Intel Mac (load average about 5), with the `pane` profile, 200 by 60 cells of 16 by 32 pixels, scale 2: hover p95 3.9 ms, at most 21 KB per hover update, 18.8 ms p95 per card after a click, 78 ms for the eight-card redraw, 144 ms from start to the first frame, 19 MB resident and 0.05% idle CPU for the dashboard; 138 ms p95 per scroll step and 65 MB resident for the table stress scene. No terminal decoded or drew those pictures, so these numbers leave out the terminal's own cost, and they say nothing about flicker; the Ghostty run decides.
- This Mac has neither Ghostty nor kitty, so the go or no-go is taken on another machine with the commands in `.agents/skills/verify-tiles/features/pane.md`.
- `EnginePainter::lay_out` now holds the layout `paint` used to do inline, and `paint` draws what it returns; the inline card's pixels are unchanged, as every golden shows.
