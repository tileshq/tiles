# lollipop form

`tiles render` draws an intent with `"form": "lollipop"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/lollipop.json` ("Billing sync fails most often on retries"), with fields `x` = `step`, `y` = `failures` and highlight `Retry`.

## Sub-features

- `lollipop-light` draws the fixture in the light theme.
- `lollipop-dark` draws it in the dark theme with the same geometry.
- `lollipop-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `lollipop-frames` writes the 10-frame entrance ending on the settled card.
- `lollipop-ranking` covers ranking sort.
- `lollipop-tone` covers tone bad.
- `lollipop-average` covers the dashed average rule: the rows' mean, or the intent's `average` column.
- `lollipop-average-clear` covers a value label written past the average rule instead of across it (ADR-051).
- `lollipop-one-row` covers a one-row lollipop with no `average` field, which draws no average of itself (ADR-051).

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `lollipop`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> lollipop-light render crates/tiles-engine/tests/intents/lollipop.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `lollipop form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Six ranked stems with dots: Retry first and red (its value 132 in ink), then Fetch, Parse, Auth, Write, Notify with grey stems, slightly darker grey heads and muted values. A grey dashed rule stands at the rows' mean, named `Average 57` above the rows, and a 0 to 140 value axis runs under them.
- **Dark card.** Run the same with name `lollipop-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `lollipop-obsidian` and `--theme obsidian`. The transcript names `lollipop form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Value past the rule.** Run the light command with name `lollipop-average-clear` on `crates/tiles-engine/tests/intents/worst-case/lollipop-worst.json`. Exit 0. A dashed rule stands at `Average 36.1%`; Aleksandra Wiśniewska-Kowalczyk's head sits left of it at 12%, and its `12.0%` is written just right of the rule, not across it.
- **One row.** Run the light command with name `lollipop-one-row` on `crates/tiles-engine/tests/intents/worst-case/lollipop-one-row.json`. Exit 0. One stem to `$12.35M`, over a `$0` to `$14M` axis, with no dashed rule and no `Average` name.
- **Entrance.** Run the light command with name `lollipop-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
