# diverging-bar form

`tiles render` draws an intent with `"form": "diverging-bar"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/diverging-bar.json` ("Three markets are above the 2.0% refund target"), with fields `x` = `market`, `y` = `gap`, highlight `Netherlands`, `Germany` and `Belgium` and a bad tone.

## Sub-features

- `diverging-bar-light` draws the fixture in the light theme.
- `diverging-bar-dark` draws it in the dark theme with the same geometry.
- `diverging-bar-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `diverging-bar-frames` writes the entrance ending on the settled card.
- `diverging-bar-sort` covers the largest-first order.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `diverging-bar`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> diverging-bar-light render crates/tiles-engine/tests/intents/diverging-bar.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `diverging-bar form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Eight bars meeting at an upright muted zero rule, largest first: Netherlands to Poland. The three above zero run right in red with `+0.6`, `+0.3`, `+0.1` to their right in ink; the five below run left in grey with `−0.2` to `−0.9` to their left in muted grey. There is no axis.
- **Dark card.** Run the same with name `diverging-bar-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `diverging-bar-obsidian` and `--theme obsidian`. The transcript names `diverging-bar form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `diverging-bar-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
