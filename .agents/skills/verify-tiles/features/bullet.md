# bullet form

`tiles render` draws an intent with `"form": "bullet"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/bullet.json` ("Three of four teams are on track for quota"), with fields `x` = `team`, `y` = `booked`, `target` = `quota` and highlight `SMB`.

## Sub-features

- `bullet-light` draws the fixture in the light theme.
- `bullet-dark` draws it in the dark theme with the same geometry.
- `bullet-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `bullet-frames` writes the 10-frame entrance ending on the settled card.
- `bullet-target` covers target tick.
- `bullet-percent` covers percent of target.
- `bullet-bands` covers the three bands that grade each target.
- `bullet-summed` draws one row per team when teams repeat, with booked and quota summed.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `bullet`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> bullet-light render crates/tiles-engine/tests/intents/bullet.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `bullet form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Four rows, each a thin bar on three grey bands (under 70%, 70 to 90% and 90 to 115% of its quota) with a dark target tick, and its percent of target past the bands. SMB's bar and its 84% are blue; the other bars are dark grey with muted percents. A value axis (0 to 2,500) runs under the rows.
- **Dark card.** Run the same with name `bullet-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `bullet-obsidian` and `--theme obsidian`. The transcript names `bullet form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `bullet-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.
- **Repeated teams.** Write the fixture with its four rows repeated twice to `$OUT/../bullet-repeat.json` and render it with name `bullet-summed`. Exit 0; `card.png` has four rows, Enterprise reads `92%` against its summed 4,000 target, and the percentages match the fixture's.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
