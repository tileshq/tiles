# Gallery cases: units, notes, axis titles, time axes and long column runs

The gallery intents in `crates/tiles-engine/tests/intents/gallery/` carry gallery cards' own rows. They drive the intent's optional `unit`, `note` and `axis` keys (ADR-012, ADR-066) and the layouts the gallery uses for long runs and months.

## Sub-features

- `gallery-column-note` draws twelve columns with a light axis, only the named value written, and a note.
- `gallery-dumbbell-note` draws the named row first with the note in place of its second value.
- `gallery-waterfall-eur` writes every step in euros, compact.
- `gallery-multi-line-months` writes percent ticks, monthly periods as a thinned time axis, and names at the line ends.
- `gallery-note-dip` sets a note by its own column, never over a neighbour's value, and refuses it where no such spot exists.
- `gallery-note-refused` refuses a note on a form that cannot place it.
- `gallery-bullet-axis` writes the bullet's thousands axis in full with `€ thousand` under it.
- `gallery-lollipop-eur` writes `Average €103` over a euro axis.
- `gallery-axis-refused` refuses an axis title on a form with no value axis along the bottom.

## How to get to it (user POV)

- Add `"unit": "percent"` (or `eur`, `usd`, `gbp`) to an intent to have every value and tick carry it.
- Add `"note": "<at most 48 characters>"` with a `highlight` to write a short callout beside the named mark.
- Add `"axis": "<at most 40 characters>"` to title the value axis of a lollipop, dumbbell, bullet, histogram, box plot or range bands.
- Run `tiles render <intent.json> -o <out.png>` as for any form.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- The run id is new, so `target/verify/<run>/` holds only this run's evidence.

- **Column with a note.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> gallery-column-note render crates/tiles-engine/tests/intents/gallery/column-highlight.json -o '$OUT/card.png'`. Exit 0. Open `card.png`: twelve columns Jan to Dec over a y axis reading 0, 1k, 2k, 3k with faint gridlines; only November is red and only its value 3,420 is written; "Checkout bug, 4 to 9 Nov" sits muted to the left of November's top, clear of every other column.
- **Dumbbell with a note.** Run the same with name `gallery-dumbbell-note` and `crates/tiles-engine/tests/intents/gallery/dumbbell.json`. APAC is the first row, its second dot red on a pale red bar with "Still 52 min" beside it in dark type; the other rows follow by gap, EMEA North first, their second dots blue. `Q3` and `Q2` are written over EMEA North's dots, and a 0 to 70 axis runs under the rows with `Minutes` centred under it.
- **Waterfall in euros.** Run the same with name `gallery-waterfall-eur` and `crates/tiles-engine/tests/intents/gallery/waterfall.json`. Steps read +€8.2M, +€1.1M, +€0.6M, −€0.2M, −€0.5M and the total €9.2M.
- **Multi-line with months.** Run the same with name `gallery-multi-line-months` and `crates/tiles-engine/tests/intents/gallery/multi-line.json`. Ticks read 25% to 40%; the x axis reads Oct 25, Jan 26, Apr 26, Jul 26; Germany is blue and named at its line's end, the other four markets muted grey and named at theirs, with no legend.
- **Note on a dip.** Write the column fixture with a note: `python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["note"]="After the launch"; json.dump(v, open(sys.argv[2],"w"))' crates/tiles-engine/tests/intents/column.json target/verify/<run>/dip.json`. Run drive.sh with name `gallery-note-dip` and `render target/verify/<run>/dip.json --width 640 -o '$OUT/card.png'`. Exit 0. Open `card.png`: "After the launch" sits muted directly above the red W3's 298, not over W2's 431 or any other column's value. At `--width 340` the same intent is refused in one line naming 340 px.
- **Refused note.** Write the stacked-bar fixture with a note added to `target/verify/<run>/note.json`: `python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); v["note"]="Card leads"; json.dump(v, open(sys.argv[2],"w"))' crates/tiles-engine/tests/intents/stacked-bar.json target/verify/<run>/note.json`. Then run drive.sh with name `gallery-note-refused` and `render target/verify/<run>/note.json -o '$OUT/card.png'`. Exit non-zero; the transcript's one line reads `note: form stacked-bar does not draw a note; drop note, or use column, bar, lollipop, dot-plot, line, area, multi-line or dumbbell`; no `card.png`.
- **Bullet axis.** Run drive.sh with name `gallery-bullet-axis` and `render crates/tiles-engine/tests/intents/gallery/bullet.json -o '$OUT/card.png'`. Exit 0. The value axis reads 0, 1,000, 2,000, 3,000 (not 1k) with `€ thousand` centred under it; EMEA and APAC are red.
- **Lollipop in euros.** Run the same with name `gallery-lollipop-eur` and `crates/tiles-engine/tests/intents/gallery/lollipop.json`. The dashed rule is named `Average €103`, Travel's stem is the accent and every value and tick carries €.
- **Refused axis title.** Write the column fixture with `"axis": "Signups"` to `target/verify/<run>/axis.json` as the note case does, then run drive.sh with name `gallery-axis-refused` and `render target/verify/<run>/axis.json -o '$OUT/card.png'`. Exit non-zero; the one line reads `axis: form column draws no value axis along the bottom; drop axis and name the unit in the subtitle`; no `card.png`.
