# area-labelled form

The opt-in variant of `area` that edges the fill with its line and writes the value at the highlighted period. `tiles render` draws an intent with `"form": "area-labelled"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/area-labelled.json` ("Storage use doubled after the June launch"), with fields `x` = `month`, `y` = `terabytes` and highlight `Jun`.

## Sub-features

- `area-labelled-light` draws the fixture in the light theme.
- `area-labelled-dark` draws it in the dark theme with the same geometry.
- `area-labelled-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `area-labelled-frames` writes the 10-frame entrance ending on the settled card.
- `area-labelled-zero` covers zero baseline.
- `area-labelled-highlighted` covers the variant's own mark: the highlighted point with its value written above it.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `area-labelled`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> area-labelled-light render crates/tiles-engine/tests/intents/area-labelled.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `area-labelled form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. The same area as the `area` form, edged with a 2.2 px line, with the June point larger and its value 58 written above it in the accent. This is the opt-in variant of the approved area card.
- **Dark card.** Run the same with name `area-labelled-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `area-labelled-obsidian` and `--theme obsidian`. The transcript names `area-labelled form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `area-labelled-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
