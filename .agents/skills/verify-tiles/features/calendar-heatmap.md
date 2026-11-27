# calendar-heatmap form

`tiles render` draws an intent with `"form": "calendar-heatmap"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/calendar-heatmap.json` ("Monday 9 to 11am is the worst two hours of the support week"), contacts per hour of an average week with fields `x` = `hour`, `row` = `day`, `y` = `contacts`, highlight `Mon`, `09:00` and `10:00` and a bad tone.

## Sub-features

- `calendar-heatmap-light` draws the fixture in the light theme.
- `calendar-heatmap-dark` draws it in the dark theme with the same geometry.
- `calendar-heatmap-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `calendar-heatmap-frames` writes the entrance ending on the settled card.
- `calendar-heatmap-rows` refuses more than 31 rows.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `calendar-heatmap`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> calendar-heatmap-light render crates/tiles-engine/tests/intents/calendar-heatmap.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `calendar-heatmap form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. A grid of 7 rows (Mon to Sun) by 15 hours (07:00 to 21:00) with every other hour named under it. Each cell is the track's shade with the accent over it by value; Monday 09:00 and 10:00 are the darkest on the light card (the lightest on a dark one) and carry `154` and `136`. `Mon` is bold ink, the other days muted. A key under the grid reads `0`, five shades and `154`.
- **Dark card.** Run the same with name `calendar-heatmap-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `calendar-heatmap-obsidian` and `--theme obsidian`. The transcript names `calendar-heatmap form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `calendar-heatmap-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **Too many rows.** Write 32 distinct `day` values to `$OUT/../calendar-heatmap-32.json` and render it with name `calendar-heatmap-rows`. Exit 1 and the refusal says `more than 31 distinct rows, more than form calendar-heatmap draws`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
