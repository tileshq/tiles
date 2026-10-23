# slope form

`tiles render` draws an intent with `"form": "slope"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/slope.json` ("Search overtook paid social as the top source"), with fields `x` = `period`, `y` = `share`, `series` = `source` and highlight `Search`.

## Sub-features

- `slope-light` draws the fixture in the light theme.
- `slope-dark` draws it in the dark theme with the same geometry.
- `slope-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `slope-frames` writes the 10-frame entrance ending on the settled card.
- `slope-series` covers series highlight.
- `slope-label` covers label spreading.
- `slope-refusals` refuses a period as highlight, a single period, and series with no value in both end periods.
- `slope-long-series` cuts long series names short so both end labels and the lines fit the card.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `slope`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> slope-light render crates/tiles-engine/tests/intents/slope.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `slope form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Two columns headed 2025 and 2026 in dark semibold type, with a line per source and no vertical guides. Search is blue and climbs from 18 to 27, drawn thicker than the rest with larger end dots and both its labels in blue semibold; the other lines are thinner, each in its own grey, with smaller dots and grey labels. The left side reads source and value ("Search  18"); the right side reads the value alone (27), firmer; no labels overlap.
- **Dark card.** Run the same with name `slope-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `slope-obsidian` and `--theme obsidian`. The transcript names `slope form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `slope-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Refusals.** Render the fixture with `"highlight": ["2025"]`, name `slope-refusals`. Exit 1 and `tiles: highlight[0]: "2025" occurs in no row of source; ...`: slope colours series, not periods. Drop every 2026 row: `tiles: data.rows: slope needs at least two periods in x; add rows for a second period`.
- **Long series name.** Rename `Search` to a 120-character name in both rows and in the highlight, name `slope-long-series`. Exit 0; the name ends in an ellipsis on both sides, every label stays inside the card and the lines keep most of the width.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
