# box-plot form

`tiles render` draws an intent with `"form": "box-plot"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/box-plot.json` ("Enterprise deals take six times longer to close than SMB deals"), 40 deals per segment with fields `x` = `segment`, `y` = `days`, highlight `Enterprise` and `axis` = `Days to close`.

## Sub-features

- `box-plot-light` draws the fixture in the light theme.
- `box-plot-dark` draws it in the dark theme with the same geometry.
- `box-plot-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `box-plot-frames` writes the entrance ending on the settled card.
- `box-plot-one-value` draws a narrow box for a segment with one value.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `box-plot`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> box-plot-light render crates/tiles-engine/tests/intents/box-plot.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `box-plot form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Three rows, SMB, Mid-market and Enterprise in data order, over a 0 to 300 value axis titled `Days to close`. Each row has a box from its 25th to its 75th percentile with a median line in the card's colour across it and thin muted whiskers; Enterprise's box is the accent and the others grey. SMB has one muted dot past its whisker (45 days).
- **Dark card.** Run the same with name `box-plot-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `box-plot-obsidian` and `--theme obsidian`. The transcript names `box-plot form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `box-plot-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **One value.** Write an intent with one row (`segment` Solo, `days` 4) to `$OUT/../box-plot-one.json` and render it with name `box-plot-one-value`. Exit 0; a narrow accent box with no median line stands at 4.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
