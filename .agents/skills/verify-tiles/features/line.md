# line form

`tiles render` draws an intent with `"form": "line"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/line.json` ("Weekly active teams climbed all year"), with fields `x` = `month`, `y` = `teams` and no highlight.

## Sub-features

- `line-light` draws the fixture in the light theme.
- `line-dark` draws it in the dark theme with the same geometry.
- `line-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `line-frames` writes the 10-frame entrance ending on the settled card.
- `line-y` covers y axis with gridlines.
- `line-point` covers point per value.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `line`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> line-light render crates/tiles-engine/tests/intents/line.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `line form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. One blue line, a touch under full strength, with a small translucent point per month Jan to Dec, horizontal gridlines with tick labels 1,200 to 1,700 on the left, month labels flat underneath. No highlight, so everything is the accent.
- **Dark card.** Run the same with name `line-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `line-obsidian` and `--theme obsidian`. The transcript names `line form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `line-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
