# range-bands form

`tiles render` draws an intent with `"form": "range-bands"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/range-bands.json` ("Card and wallet payouts land within a day; transfers and BNPL take up to five"), with fields `x` = `method`, `lo` = `p5`, `hi` = `p95`, `mid` = `p50`, `inner_lo` = `p25`, `inner_hi` = `p75`, highlight `SEPA transfer` and `Buy now pay later`, a bad tone and `axis` = `Hours to payout`.

## Sub-features

- `range-bands-light` draws the fixture in the light theme.
- `range-bands-dark` draws it in the dark theme with the same geometry.
- `range-bands-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `range-bands-frames` writes the entrance ending on the settled card.
- `range-bands-bounds` refuses a row whose lo is above its hi.
- `range-bands-nest` refuses a row whose inner band or middle dot falls outside its outer band.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `range-bands`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> range-bands-light render crates/tiles-engine/tests/intents/range-bands.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `range-bands form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Five rows on faint row lines over a 0 to 120 value axis titled `Hours to payout`. Each row has a light pill from p5 to p95, a darker pill from p25 to p75 inside it and an ink dot at the median. SEPA transfer and Buy now pay later are red; the other three are grey.
- **Dark card.** Run the same with name `range-bands-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `range-bands-obsidian` and `--theme obsidian`. The transcript names `range-bands form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `range-bands-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **Bounds out of order.** Swap `p5` and `p95` in the first row, name `range-bands-bounds`. Exit 1 and `tiles: data.rows[0]: lo is above hi; send the lower bound in lo and the upper in hi`.
- **Bands that do not nest.** Set `p25` in the first row to 4 (below its `p5` of 6), name `range-bands-nest`. Exit 1 and `tiles: data.rows[0]: inner_lo is below lo; send inner_lo between lo and hi, as the bands nest`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
