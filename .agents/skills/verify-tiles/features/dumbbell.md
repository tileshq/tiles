# dumbbell form

`tiles render` draws an intent with `"form": "dumbbell"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/dumbbell.json` ("Routing got faster in every queue, most in Billing"), with fields `x` = `queue`, `lo` = `before`, `hi` = `after` and highlight `Billing`.

## Sub-features

- `dumbbell-light` draws the fixture in the light theme.
- `dumbbell-dark` draws it in the dark theme with the same geometry.
- `dumbbell-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `dumbbell-frames` writes the 10-frame entrance ending on the settled card.
- `dumbbell-gap` covers gap sort.
- `dumbbell-highlight` covers highlight.
- `dumbbell-summed` draws one row per queue when queues repeat, with before and after summed.
- `dumbbell-incomplete` refuses rows that never hold both numbers.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `dumbbell`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> dumbbell-light render crates/tiles-engine/tests/intents/dumbbell.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `dumbbell form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Five rows on faint row gridlines over a 0 to 50 minutes axis, Billing first, then by change. Each row joins a grey before dot to an after dot; only Billing's after dot (18) is blue, the other rows fade to grey. `before` and `after` are written once, each over its own dot of Billing's row. No values are written beside the dots; the axis carries them.
- **Dark card.** Run the same with name `dumbbell-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `dumbbell-obsidian` and `--theme obsidian`. The transcript names `dumbbell form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `dumbbell-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Repeated queues.** Write the fixture with its five rows repeated 40 times to `$OUT/../dumbbell-repeat.json` and render it with name `dumbbell-summed`. Exit 0; `card.png` still has five rows, the axis runs to 40 times the fixture's (2,000), and the card is no taller than the fixture's.
- **No complete row.** Set `after` to null in every row with `before` set, and the reverse, name `dumbbell-incomplete`. Exit 1 and `tiles: data.rows: no row holds both a lo and a hi number; fill both columns in at least one row`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
