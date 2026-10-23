# big-number form

`tiles render` draws an intent with `"form": "big-number"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/big-number.json` ("Annual recurring revenue reached $4.82M"), with fields `x` = `quarter`, `y` = `arr`, `target` = `goal` and no highlight.

## Sub-features

- `big-number-light` draws the fixture in the light theme.
- `big-number-dark` draws it in the dark theme with the same geometry.
- `big-number-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `big-number-frames` writes the 10-frame entrance ending on the settled card.
- `big-number-headline` covers headline value.
- `big-number-start` covers the sparkline's start label: the first period, or `12 months ago` for `YYYY-MM` rows a year apart, counted from the first period to the last.
- `big-number-target` covers the target line and its progress bar, drawn only when the intent has a `target`.
- `big-number-sparkline` covers sparkline.
- `big-number-highlight-ignored` draws a card with a highlight the same as one without: the big number has nothing to colour, so it ignores the highlight rather than refusing it.
- `big-number-compact` draws a big number among other cards of a `tiles_chart` row compact: the one-line title, the number, its change line and a 26 px sparkline under them, no period labels and no default subtitle. A lone big number is the gallery card above. Driven through `mcp-server`'s row.
- `big-number-thousands` writes a headline between 1,000 and a million with a K step: 262000 reads 262K.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `big-number`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> big-number-light render crates/tiles-engine/tests/intents/big-number.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `big-number form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. A large 4.82M (34 px) at the top left, `96% of the 5M target` under it and a 6 px rounded grey track under that, filled 96% in blue. Under the bar a blue sparkline with a light fill rises from `Q1 25` (written in grey under its first point) to a dot at Q3 26, stopping short of the card's right edge. No change line and no axis. Without a `target`, the card is the gallery's big number: the number over the sparkline, no target line and no bar.
- **Dark card.** Run the same with name `big-number-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `big-number-obsidian` and `--theme obsidian`. The transcript names `big-number form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `big-number-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Highlight ignored.** Add `"highlight": ["Q3 26"]` to the fixture and render it with name `big-number-highlight-ignored`. Exit 0, and `card.png` is byte for byte the light card. Through the MCP server the result text adds `Form big-number has no category to colour, so Tiles ignored highlight.`
- **Thousands.** Change the fixture's `arr` values to 249000 and 262000 (two rows) with `"unit": "usd"`, name `big-number-thousands`. Exit 0; the card reads $262K, with no change line. With 999600 as the last value it reads $1M, not $1,000K.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
