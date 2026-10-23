# column form

`tiles render` draws an intent with `"form": "column"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/column.json` ("Signups dipped in W3"), with fields `x` = `week`, `y` = `signups` and highlight `W3`.

## Sub-features

- `column-light` draws the fixture in the light theme.
- `column-dark` draws it in the dark theme with the same geometry.
- `column-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `column-frames` writes the 10-frame entrance ending on the settled card.
- `column-highlight` covers highlight.
- `column-tone` covers tone bad.
- `column-zero` covers zero baseline.
- `column-values` covers values on bars.
- `column-long-label` covers one long category name, which hides only the labels it would touch (ADR-051).

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `column`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> column-light render crates/tiles-engine/tests/intents/column.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `column form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Six vertical bars W1 to W6 in data order, each with its value above it (412, 431, 298, 447, 463, 489). Only the W3 bar is red, and its value 298 is red; the other bars are grey with muted values. No y axis.
- **Dark card.** Run the same with name `column-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `column-obsidian` and `--theme obsidian`. The transcript names `column form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **One long name.** Run the light command with name `column-long-label` on `crates/tiles-engine/tests/intents/worst-case/column-worst.json`. Exit 0. Under the six columns, Benachrichtigungseinstellungen sits at the card's left edge, then نور الهدى عبد الرحمن, 🦊 Fox and 👩🏽 Priya are named (empty boxes stand in for the Arabic and emoji glyphs off macOS); only 王秀英 and Aleksandra Wiśniewska-Kowalczyk, which would touch their neighbours, are left out.
- **Entrance.** Run the light command with name `column-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
