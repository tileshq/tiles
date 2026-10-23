# dot-plot form

`tiles render` draws an intent with `"form": "dot-plot"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/dot-plot.json` ("Support replies within 4 hours in every region but APAC"), with fields `x` = `region`, `y` = `hours` and highlight `APAC`.

## Sub-features

- `dot-plot-light` draws the fixture in the light theme.
- `dot-plot-dark` draws it in the dark theme with the same geometry.
- `dot-plot-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `dot-plot-frames` writes the 10-frame entrance ending on the settled card.
- `dot-plot-non-zero` covers non-zero domain.
- `dot-plot-tone` covers tone bad.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `dot-plot`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> dot-plot-light render crates/tiles-engine/tests/intents/dot-plot.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `dot-plot form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Five ranked dots on faint tracks, APAC first with a red dot and its value 5.6 in ink at regular weight; the others grey with muted values. The axis does not start at zero.
- **Dark card.** Run the same with name `dot-plot-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `dot-plot-obsidian` and `--theme obsidian`. The transcript names `dot-plot form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `dot-plot-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
