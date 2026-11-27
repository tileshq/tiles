# small-multiples form

`tiles render` draws an intent with `"form": "small-multiples"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/small-multiples.json` ("Win rate held steady in every segment except mid-market"), eight quarters per segment with fields `x` = `quarter`, `y` = `win_rate`, `series` = `segment`, unit percent, highlight `Mid-market` and a bad tone.

## Sub-features

- `small-multiples-light` draws the fixture in the light theme.
- `small-multiples-dark` draws it in the dark theme with the same geometry.
- `small-multiples-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `small-multiples-frames` writes the entrance ending on the settled card.
- `small-multiples-too-many` refuses more than 12 panels.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `small-multiples`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> small-multiples-light render crates/tiles-engine/tests/intents/small-multiples.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `small-multiples form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Four panels two by two: SMB, Mid-market, Enterprise and Partner-sourced, each named above its plot and each reading `0%`, `20%`, `40%` on its left and `Q1 25` and `Q4 26` under it. Each panel draws its own line over the other three in faint grey; Mid-market's line, name and end point are red and end at `19%`, the other three lines muted grey ending at `30%`, `23%` and `36%`.
- **Dark card.** Run the same with name `small-multiples-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `small-multiples-obsidian` and `--theme obsidian`. The transcript names `small-multiples form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `small-multiples-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **Too many panels.** Write 13 segments with two quarters each to `$OUT/../small-multiples-13.json` and render it with name `small-multiples-too-many`. Exit 1 and the refusal starts `tiles: fields.series: 13 panels is more than form small-multiples draws (12)`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
