# grouped-bar form

`tiles render` draws an intent with `"form": "grouped-bar"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/grouped-bar.json` ("Every region but South closed more deals in 2026"), with fields `x` = `region`, `y` = `deals`, `series` = `year` and no highlight.

## Sub-features

- `grouped-bar-light` draws the fixture in the light theme.
- `grouped-bar-dark` draws it in the dark theme with the same geometry.
- `grouped-bar-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `grouped-bar-frames` writes the 10-frame entrance ending on the settled card.
- `grouped-bar-categorical` covers categorical order.
- `grouped-bar-values` covers values on bars.
- `grouped-bar-legend-wrap` wraps a legend of many or long series onto more rows.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `grouped-bar`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> grouped-bar-light render crates/tiles-engine/tests/intents/grouped-bar.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `grouped-bar form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Four region groups with a 2025 bar (blue) and a 2026 bar (dark grey) each, values above every bar, a legend with both years.
- **Dark card.** Run the same with name `grouped-bar-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `grouped-bar-obsidian` and `--theme obsidian`. The transcript names `grouped-bar form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `grouped-bar-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Wrapped legend.** Follow `render-legend-wrap` in `cli-render.md` with name `grouped-bar-legend-wrap`. Every legend entry sits inside the card on two or more rows, and the columns start below the last row.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
