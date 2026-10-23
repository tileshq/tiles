# multi-line form

`tiles render` draws an intent with `"form": "multi-line"` as a card PNG. The fixture intent is `crates/tiles-engine/tests/intents/multi-line.json` ("Self-serve win rate rose while sales-led held flat"), with fields `x` = `month`, `y` = `win_rate`, `series` = `channel` and highlight `Self-serve`.

## Sub-features

- `multi-line-light` draws the fixture in the light theme.
- `multi-line-dark` draws it in the dark theme with the same geometry.
- `multi-line-obsidian` draws it in Obsidian, the default theme, with the same geometry.
- `multi-line-frames` writes the 10-frame entrance ending on the settled card.
- `multi-line-series` covers series highlight.
- `multi-line-legend` covers the series names: written at each line's end, with a legend only when they cannot fit down the plot.
- `multi-line-scripts` covers Chinese, Arabic and emoji series names drawn with the macOS system fonts, and a long Arabic name cut on its left (ADR-050).

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png>` with an intent whose `form` is `multi-line`.
- Cards are drawn in Obsidian unless `--theme light` or `--theme dark` (or a theme saved with `tiles theme`) picks another; add `--frames <dir>` for the entrance.
- Run `tiles forms` to read the roles this form takes.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Light card.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> multi-line-light render crates/tiles-engine/tests/intents/multi-line.json -o '$OUT/card.png' --theme light`. Exit 0; the transcript names `multi-line form, light theme, 1280x` and the PNG is 1280 px wide.
- **What the card shows.** Open `card.png`. Three lines Jan to Jun, no legend. Each line is named just past its right end: Self-serve in blue at semibold, Sales-led and Partner muted, the names not overlapping. The Self-serve line (3 px) is blue; Sales-led and Partner are thinner grey lines (1.8 px) in two different greys, Sales-led the darker in light and the lighter in dark (ADR-047). No line carries points, since a series is spotlit.
- **Dark card.** Run the same with name `multi-line-dark` and `--theme dark`. The card is near-black with light text; the same mark carries the (lighter) accent and the PNG has the same pixel size as the light one.
- **Obsidian card.** Run the same with name `multi-line-obsidian` and `--theme obsidian`. The transcript names `multi-line form, obsidian theme`; the card is graphite with light text, the accent is purple and the PNG has the same pixel size as the light one.
- **Scripts.** On a Mac, run `.agents/skills/verify-tiles/scripts/drive.sh <run> multi-line-scripts render crates/tiles-engine/tests/intents/worst-case/multi-line-worst.json -o '$OUT/scripts.png'`, then the same with `worst-case/multi-line-rtl.json` and `-o '$OUT/rtl.png'`. Both exit 0. In `scripts.png` the names 王秀英, نور الهدى عبد الرحمن and 🦊 Fox draw as letters and a colour fox, with no empty boxes. In `rtl.png` the Arabic name ends in an ellipsis on its left and reads from the right starting عبد الرحمن; Aleksandra's name keeps its ellipsis on the right.
- **Entrance.** Run the light command with name `multi-line-frames` and `--frames '$OUT/frames' --timings`. `frames/` holds `frame-00.png` to `frame-09.png`; frame 00 is fully transparent, a middle frame shows the card lifting and marks part-grown, and frame 09 matches `card.png`.

## Gotchas

- Off macOS, or on a Mac missing PingFang SC, Geeza Pro or Apple Color Emoji, the scripts sub-feature draws empty boxes for that script; record it as not run there.
- The golden images in `crates/tiles-engine/tests/golden-plain/` are scale 1; the CLI draws scale 2 by default, so compare against the description above, not against a golden by size.
- A changed fixture intent changes the expected end state; update this file in the same change.
