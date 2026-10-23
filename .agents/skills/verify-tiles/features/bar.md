# bar form

`tiles render` draws an intent with `"form": "bar"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/bar.json` ("Northwind leads new revenue by a wide margin"), with fields `x` = `account`, `y` = `revenue` and highlight `Northwind`.

## Sub-features

- `bar-light` draws the fixture in the light theme.
- `bar-dark` draws it in the dark theme with the same geometry.
- `bar-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `bar-frames` writes the 10-frame entrance ending on the settled card.
- `bar-ranking` covers ranking sort.
- `bar-highlight` covers highlight.
- `bar-thousands` covers thousands separators.
- `bar-long-labels` cuts long account names short, and shows look-alike names by the word where they differ so they stay different.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `bar`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> bar-light render crates/tiles-engine/tests/intents/bar.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `bar form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Six horizontal bars ranked largest first: Northwind, Globex, Acme, Umbrella, Initech, Hooli, labels on the left. Northwind's bar is blue and its value 412,500 is in ink; the rest are grey with muted values. Values sit 4 px past each bar end at regular weight.
- **Dark card.** Run the same with name `bar-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `bar-obsidian` and `--theme obsidian`. The transcript names `bar form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `bar-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Long names.** Follow `render-long-labels` in `cli-render.md` with name `bar-long-labels`. The two look-alike names end `…Search` and `…Social` (start kept); in the `render-long-window` variant they start `…Search - all devices` and `…Social - all devices` and end in `…`. A lone long name ends in `…`, and no bar runs left of its label.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
