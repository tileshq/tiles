# waterfall form

`tiles render` draws an intent with `"form": "waterfall"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/waterfall.json` ("Expansion more than covered churn this quarter"), with fields `x` = `movement`, `y` = `arr` and no highlight.

## Sub-features

- `waterfall-light` draws the fixture in the light theme.
- `waterfall-dark` draws it in the dark theme with the same geometry.
- `waterfall-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `waterfall-frames` writes the 10-frame entrance ending on the settled card.
- `waterfall-signed` covers signed format.
- `waterfall-running` covers running total.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `waterfall`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> waterfall-light render crates/tiles-engine/tests/intents/waterfall.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `waterfall form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Floating steps New +412 and Expansion +286 in blue, Contraction −94 and Churn −187 in red, faint connectors between them, and a dark grey Total bar of 417.
- **Dark card.** Run the same with name `waterfall-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `waterfall-obsidian` and `--theme obsidian`. The transcript names `waterfall form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `waterfall-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
