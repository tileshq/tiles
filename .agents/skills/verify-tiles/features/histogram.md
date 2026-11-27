# histogram form

`tiles render` draws an intent with `"form": "histogram"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/histogram.json` ("Three in ten demo requests wait longer than the 15-minute SLA"), with fields `x` = `minute` (each bin's start), `y` = `requests` (its count), `threshold` = `sla` (15), `axis` = `Minutes to first touch` and a bad tone.

## Sub-features

- `histogram-light` draws the fixture in the light theme.
- `histogram-dark` draws it in the dark theme with the same geometry.
- `histogram-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `histogram-frames` writes the entrance ending on the settled card.
- `histogram-raw` counts values sent one a row into round bins.
- `histogram-threshold-inside` refuses a threshold that falls inside a sent bin.
- `histogram-uneven` refuses sent bin starts at uneven gaps.
- `histogram-threshold-end` draws a threshold on the last sent bin's end.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `histogram`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> histogram-light render crates/tiles-engine/tests/intents/histogram.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `histogram form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Twelve touching columns over a 0 to 60 value axis, a count axis on the left reading 0 to 400 with faint gridlines, and `Minutes to first touch` under the value axis. The three bins before 15 are grey; the nine from 15 on are red. A dashed ink rule stands at 15 with `30% at 15 or more` in red beside its top.
- **Dark card.** Run the same with name `histogram-dark` and `--theme dark`. The card is near-black with light text, the same marks carry the (lighter) accent or red, and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `histogram-obsidian` and `--theme obsidian`. The transcript names `histogram form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Entrance.** Run the light command with name `histogram-frames` and `--frames '$OUT/frames' --timings`. Frame 00 is fully transparent and the last frame matches `card.png`.
- **Raw values.** Write an intent with `form` histogram, fields `x` = `wait` and `threshold` = `limit`, and 100 rows with `wait` 0.5 to 99.5 and `limit` 37 to `$OUT/../histogram-raw.json`; render it with name `histogram-raw`. Exit 0; the bins are even, a bin edge sits on 37 and the label reads `63% at 37 or more`.
- **Threshold inside a bin.** Render the fixture with every `sla` set to 17, name `histogram-threshold-inside`. Exit 1 and `tiles: fields.threshold: the threshold falls inside a bin, so the share past it cannot be counted; send a threshold on a bin edge (a start or the last bin's end), or leave y out and send the values`.
- **Uneven bins.** Write an intent with `form` histogram, fields `x` = `from`, `y` = `n` and `threshold` = `limit`, and six rows with `from` 0, 5, 10, 15, 30 and 60, `n` 10 each and `limit` 30 to `$OUT/../histogram-uneven.json`; render it with name `histogram-uneven`. Exit 1 and `tiles: data.rows: sent bins must be equal width; send bin starts an equal step apart, with a row of y 0 for an empty bin, or leave y out and send the values`.
- **Threshold at the last bin's end.** Render the fixture with every `sla` set to 60, name `histogram-threshold-end`. Exit 0; no bin is coloured and the label reads `0% at 60 or more`.

## Gotchas

- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
