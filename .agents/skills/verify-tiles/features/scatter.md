# scatter form

`tiles render` draws an intent with `"form": "scatter"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/scatter.json` ("Larger accounts adopt more features, Globex is the outlier"), with fields `x` = `seats`, `y` = `features`, `label` = `account` and highlight `Globex`.

## Sub-features

- `scatter-light` draws the fixture in the light theme.
- `scatter-dark` draws it in the dark theme with the same geometry.
- `scatter-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `scatter-frames` writes the 10-frame entrance ending on the settled card.
- `scatter-label` covers label highlight.
- `scatter-two` covers two numeric axes.
- `scatter-incomplete` refuses data where no row holds both measures.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `scatter`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> scatter-light render crates/tiles-engine/tests/intents/scatter.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `scatter form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Ten points on seat count by features, gridlines and tick labels on both axes. Globex is red with its name beside it, inside the plot and clear of the x-axis numbers; no point sits on the plot's edge. Every other point is grey.
- **Dark card.** Run the same with name `scatter-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `scatter-obsidian` and `--theme obsidian`. The transcript names `scatter form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `scatter-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **No complete row.** Set `features` to null in every row where `seats` is set and the reverse, name `scatter-incomplete`. Exit 1 and `tiles: data.rows: no row holds both an x and a y number; fill both columns in at least one row`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
