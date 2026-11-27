# bump form

`tiles render` draws an intent with `"form": "bump"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/bump.json` ("Wallets climbed from 4th to 1st in payment volume this year"), monthly volume per payment method with fields `x` = `month`, `y` = `volume`, `series` = `method` and highlight `Wallets`; Tiles ranks each month largest first.

## Sub-features

- `bump-light` draws the fixture in the light theme.
- `bump-dark` draws it in the dark theme with the same geometry.
- `bump-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `bump-frames` writes the entrance ending on the settled card.
- `bump-one-period` refuses rows that cover one period.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `bump`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> bump-light render crates/tiles-engine/tests/intents/bump.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `bump form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Ranks `#1` to `#5` down the left and months Jan to Sep along the bottom. Wallets is a firm accent line with large points, from #4 in January to #1 in August and September, named in bold accent at its end. The other four are thin context-grey lines with small points, each named in muted grey at its end.
- **Dark card.** Run the same with name `bump-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `bump-obsidian` and `--theme obsidian`. The transcript names `bump form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `bump-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **One period.** Keep only the January rows, name `bump-one-period`. Exit 1 and `tiles: data.rows: bump needs at least two periods in x with a y number; add rows for a second period, or use bar to compare one period`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
