# stacked-bar form

`tiles render` draws an intent with `"form": "stacked-bar"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/stacked-bar.json` ("Enterprise accounts nearly doubled over the year"), with fields `x` = `quarter`, `y` = `accounts`, `series` = `plan` and highlight `Enterprise`.

## Sub-features

- `stacked-bar-light` draws the fixture in the light theme.
- `stacked-bar-dark` draws it in the dark theme with the same geometry.
- `stacked-bar-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `stacked-bar-frames` writes the 10-frame entrance ending on the settled card.
- `stacked-bar-series` covers series highlight.
- `stacked-bar-totals` covers totals.
- `stacked-bar-x-highlight` refuses a quarter as highlight: stacks colour by series only.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `stacked-bar`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> stacked-bar-light render crates/tiles-engine/tests/intents/stacked-bar.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `stacked-bar form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Four stacked columns Q1 to Q4 with totals 206, 234, 265, 302 above. The Enterprise segment on top of each stack is blue; Starter (bottom) and Growth (middle) take two different neutral greys, darker first, and the legend swatches match the segments.
- **Dark card.** Run the same with name `stacked-bar-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `stacked-bar-obsidian` and `--theme obsidian`. The transcript names `stacked-bar form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `stacked-bar-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Quarter as highlight.** Render the fixture with `"highlight": ["Q3"]`, name `stacked-bar-x-highlight`. Exit 1 and `tiles: highlight[0]: "Q3" occurs in no row of plan; name values exactly as they appear in the data, or drop highlight`; no card is written.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
