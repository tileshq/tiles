# funnel form

`tiles render` draws an intent with `"form": "funnel"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/funnel.json` ("Four in ten shoppers give up at the payment-details step"), with fields `x` = `step`, `y` = `shoppers`, highlight `Details entered` and a bad tone.

## Sub-features

- `funnel-light` draws the fixture in the light theme.
- `funnel-dark` draws it in the dark theme with the same geometry.
- `funnel-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `funnel-frames` writes the entrance ending on the settled card.
- `funnel-negative` refuses a step with a negative count.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `funnel`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> funnel-light render crates/tiles-engine/tests/intents/funnel.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `funnel form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Five bars in the order sent, Checkout started to Paid, with no axis. Each bar after the first sits on a light ghost of the bar before it. The values read `10,000`, `8,200 (82%)`, `5,100 (62%)`, `4,700 (92%)` and `4,550 (97%)`; Details entered and its label are red, the other bars grey.
- **Dark card.** Run the same with name `funnel-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `funnel-obsidian` and `--theme obsidian`. The transcript names `funnel form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `funnel-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **Negative count.** Set the last step's `shoppers` to -1, name `funnel-negative`. Exit 1 and `tiles: data.rows: funnel needs values of zero or more; send how many reached each step, a count of zero or more`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
