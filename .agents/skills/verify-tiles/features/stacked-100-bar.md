# stacked-100-bar form

`tiles render` draws an intent with `"form": "stacked-100-bar"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/stacked-100-bar.json` ("Chat carries the most support in the Nordics and Benelux"), with fields `x` = `region`, `y` = `tickets`, `series` = `channel` and highlight `Chat`.

## Sub-features

- `stacked-100-bar-light` draws the fixture in the light theme.
- `stacked-100-bar-dark` draws it in the dark theme with the same geometry.
- `stacked-100-bar-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `stacked-100-bar-frames` writes the 10-frame entrance ending on the settled card.
- `stacked-100-bar-shares` covers the shares written inside their segments.
- `stacked-100-bar-order` covers rows ranked by the named series' share.
- `stacked-100-bar-ends` covers a share that only rounds to 0 or 100, written `<1` or `>99` (ADR-051).
- `stacked-100-bar-negative` refuses a negative value.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `stacked-100-bar`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> stacked-100-bar-light render crates/tiles-engine/tests/intents/stacked-100-bar.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `stacked-100-bar form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. A legend with square swatches (Chat, Email, Phone, Other), then five horizontal bars that each fill the plot from 0% to 100%: Nordics, Benelux, DACH, Iberia, Italy, ranked by Chat's share. Chat is blue with its share in white (52, 38, 31, 27, 19); Email, Phone and Other take three neutral greys, darker first, with their shares in dark ink. A thin card-coloured seam parts each segment. Shares under 10 (Nordics' and Italy's Other) stay unwritten. The axis reads 0%, 50%, 100%, its end labels flush with the bars' ends.
- **Dark card.** Run the same with name `stacked-100-bar-dark` and `--theme dark`. The card is near-black with light text; Chat takes the lighter accent with dark type on it, the greys darken and their shares turn light, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `stacked-100-bar-obsidian` and `--theme obsidian`. The transcript names `stacked-100-bar form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `stacked-100-bar-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and segments part-grown from the left, and frame 09 matches `card.png`.
- **A share short of 100.** Run the light command with name `stacked-100-bar-ends` on `crates/tiles-engine/tests/intents/worst-case/stacked-100-bar-worst.json`. Exit 0. The Benachrichtigungseinstellungen, 王秀英 and Aleksandra Wiśniewska-Kowalczyk bars read `>99`, since a sliver of the other series shares each; the نور الهدى عبد الرحمن and 👩🏽 Priya bars, whose other share is exactly 0, read `100`, and 🦊 Fox reads `91`.
- **Negative value.** Set the first row's `tickets` to -5, name `stacked-100-bar-negative`. Exit 1 and `tiles: data.rows: stacked-100-bar needs values of zero or more; use grouped-bar for signed values`; no card is written.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
