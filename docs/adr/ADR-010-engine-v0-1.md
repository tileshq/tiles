# ADR-010: The engine draws text as glyph outlines, keeps forms and tokens as checked data, ships 15 forms, and is held to the prototype by a measured distance

- Status: Accepted; amended by [ADR-012](ADR-012-prototype-parity.md); its plain embedding of forms and tokens is superseded by [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md); its entrance's frames and curves by [ADR-030](ADR-030-entrance-by-the-clock-on-every-route.md); the one grey of a spotlight line chart's unnamed lines by [ADR-047](ADR-047-context-greys-and-no-red-category.md), which gives them four ordered context greys; its "a character Inter lacks draws nothing" by [ADR-050](ADR-050-macos-system-fonts-behind-inter.md), which sets those characters with three macOS system fonts
- Date: 2026-10-18

## Context

ADR-002 fixed the pipeline (intent to scene to Skia raster) and ADR-006 the proof (golden diffs, rule tests, references from the prototype's HTML card). The engine round had to make the choices those records left open: how text stays identical on macOS and Linux, what a form file holds, which forms v0.1 ships, how close to the prototype is close enough, and what the engine costs on this Mac.

## Decision

### Text as glyph outlines

Skia's paragraph module shapes text with HarfBuzz over the three bundled Inter weights, registered as the only fonts through an asset font manager with fallback off (playbook T1). Each glyph is then taken as an unhinted outline and filled as a path by Skia's own rasterizer. Core Text (macOS) and FreeType (Linux) never rasterize a glyph, so the platform scaler cannot change a pixel, and the golden tests can hold the same per-channel tolerance of 1 on both CI runners. Tabular figures come from the `tnum` feature. A character Inter lacks draws nothing rather than a system font.

### Forms and tokens are checked data

`assets/tokens.json` holds every playbook number and both themes' colours. Each `assets/forms/<name>.json` names a form, a one-line summary, its roles (name, kind, required, meaning) and one of the core's 13 layouts (columns, bars, line, kpi, grouped, stacked, lollipop, dots, slope, dumbbell, bullet, waterfall, scatter) with its options. Loading refuses unknown keys, a role the layout does not read, and a layout role the form leaves out, so a bad form file fails at load with the file named. Both are embedded with `include_str!`; a debug build reads them from `$TILES_DEV_ASSETS` instead, and the release build has no code path that reads them from disk.

### v0.1 forms

column, bar (ranked horizontal), line, area, big-number, multi-line, grouped-bar, stacked-bar, lollipop, dot-plot, slope, dumbbell, bullet, waterfall and scatter. The engine decides what ADR-003 gives it: sort (ranked forms largest first, periods in data order), the accent only on what the highlight names, red for a bad tone, values on bars and no y axis for bar forms, a zero baseline for bars and areas, one number format per chart (separators, a true minus sign, compact M and B, at most two decimals), and balanced title wrapping. Categorical forms refuse more than 60 categories; stacked bars refuse negative values. A form also refuses a highlight it cannot colour, since that would draw a silently wrong chart. Series colours hold one invariant, checked on the resolved colours: two different series never share a colour, except the de-emphasised rest group of a spotlight line chart, whose unnamed lines all take one grey and are not limited. Every other shared colour is refused in one line: more series than the categorical palette with none named, more unnamed stacks or groups than the neutrals beside a highlight, or more than one named series, since every named series takes the accent. Dumbbell and bullet sum repeated categories into one row. Long category, series and legend labels are cut to 35% of the card's inner width with an ellipsis instead of refused, and two different categories never print the same label; look-alikes keep their start and their differing end, or show a window from the word where they differ.

### Close enough to the prototype

`tools/golden-refs/make_refs.py` feeds the engine's fixture intents through the prototype's own chart sense and HTML card in headless Chrome and commits the screenshots to `crates/tiles-engine/tests/reference/`. The prototype has four of the v0.1 forms in its core (column, ranked bar, line, area); the other eleven take the playbook as their reference and their golden is a reviewed first render. Vega and the engine measure text and place axes slightly differently, so `tests/reference.rs` measures, over both images composited on white, the mean channel difference and the share of pixels differing by more than 48, plus the height difference, and fails above bounds set at about 1.25 times the values measured on 2026-10-18 (mean 1.1 to 5.3 of 255, 0.9% to 4.3% of pixels, 0 to 4 px). Two constants in the layout (`X_LABEL_NUDGE`, `X_AXIS_EXTRA`) carry the x-axis spacing measured against those references. The ranked bar's largest distance is a deliberate difference: the playbook (K4) colours a highlighted value with its mark, where the prototype's gallery spec wrote it in ink.

### Entrance and hover from the scene

The 22 entrance frames are the scene drawn at 22 moments over 540 ms with the prototype's CSS curves: the card fades over 280 ms and floats up 14 px from a 0.985 scale over 460 ms, marks grow from their baseline (bars scale, points move, lines wipe) from 140 ms to 520 ms, values fade in over the last part, and the last frame is the settled card. The hover hit map is every data mark's box in device pixels with its `name: value` lines; a point or thin mark gets at least an 8 px target, and the smallest box under the pointer wins.

### Measured on this Mac

2026-10-18, 2017 dual-core i5-7360U, macOS 13.7.8, release build, another build running (load average 4 to 8), column and multi-line fixtures at scale 2 (1280 px wide):

| Step | Time |
|---|---|
| Engine start (tokens, 15 forms, three fonts) | 35 to 66 ms |
| One card: layout, raster and PNG encode | 50 to 61 ms (one outlier 97 ms) |
| 22 entrance frames, raw RGBA | 257 to 289 ms |
| `tiles render` wall time, one card | 0.10 s |
| Release binary, stripped, fonts embedded | 16.9 MB (16,899,768 bytes) |

## Consequences

- Text is a little lighter than Chrome's at small sizes, since outlines get no scaler gamma or contrast boost. The reference bounds include it; scale 2, the CLI default, hides it.
- A new form that fits a layout is a JSON file, a fixture intent, two golden images and a feature file in `verify-tiles`. A form that needs a new mark is Rust work in `layout/`.
- The reference bounds were measured on macOS. The engine's output is meant to be identical on Linux; the CI run on Linux is the first proof of that, and a gap there is an engine bug, not a reason to loosen a bound.
- The x-axis constants tie the layout to Vega's spacing. If the owner prefers a different spacing, the constants and the reference bounds change together in one PR.
