# Card rules

Every card Tiles draws meets these rules, whatever theme it is drawn in. Each row gives the rule's id, the rule in one line and the test that enforces it. The sizes and colours that realise a rule are the design tokens' (`crates/tiles-engine/assets/plain/tokens.json` in a build from source, the design pack's in an official build; ADR-036, ADR-037), so this page carries no values.

A rule test reads its values from the tokens and fails with the rule's id. `crates/tiles-engine/tests/themes.rs` checks every theme's colours: each categorical colour and each text colour meets a contrast floor on the card, every two categorical colours stay apart for readers with deuteranopia or protanopia, no category is red, orange or magenta, no category but the listed house greys sits near a context grey, and no series takes a text or axis colour. `scripts/check-docs.py` fails CI when a row names no test or a test that does not exist, when a row carries a value, and, while `docs/playbook.md` is present, when the two pages list different rule ids.

## Card

| # | Rule | Checked by |
|---|---|---|
| C1 | The card has round corners | test `crates/tiles-engine/tests/golden.rs` |
| C2 | Content sits inside even padding | test `crates/tiles-engine/tests/golden.rs` |
| C3 | A hairline border draws the card's edge | test `crates/tiles-engine/tests/golden.rs` |
| C4 | A shadow lifts the card | test `crates/tiles-engine/tests/golden.rs` |
| C5 | Margins leave room for the shadow, and the tile snaps to whole terminal cells | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/tests/golden.rs` |
| C6 | The card has a solid background | test `crates/tiles-engine/tests/golden.rs` |

## Type

| # | Rule | Checked by |
|---|---|---|
| T1 | Inter, bundled in the binary, for every character it has; on macOS PingFang SC, Geeza Pro and Apple Color Emoji for the rest, and no other system font (ADR-050) | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/src/text.rs` |
| T2 | The title states the finding, wrapped into balanced lines | test `crates/tiles-engine/tests/rules.rs` |
| T3 | The subtitle says what is measured, and defaults to "y by x" | test `crates/tiles-engine/tests/rules.rs` |
| T4 | Labels are small and quiet; values are small and firm | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/tests/golden.rs` |
| T5 | The chart starts a fixed distance below the text | test `crates/tiles-engine/tests/golden.rs` |
| T6 | Numbers are easy to read and compare: thousands separators, tabular figures, and no value far below the chart's compact step reads as zero (ADR-049) | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/src/format.rs` |
| T7 | Numbers carry what the measure counts, the intent's unit on every value, tick and tooltip | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/src/format.rs` |
| T8 | Monthly periods read as time, thinned to a calendar step on a point-based axis | test `crates/tiles-engine/tests/rules.rs` |

## Colour

| # | Rule | Checked by |
|---|---|---|
| K1 | The accent goes only on what the title names; every other mark is grey, and the lines beside a named line take ordered greys so each can be told apart | test `crates/tiles-engine/tests/rules.rs` |
| K2 | A bad finding colours its highlighted mark red instead of the accent, chosen by the intent's tone, and red means nothing else | test `crates/tiles-engine/tests/rules.rs` |
| K3 | A single-series chart with no highlight uses the accent on every mark | test `crates/tiles-engine/tests/rules.rs` |
| K4 | Value labels take their mark's emphasis | test `crates/tiles-engine/tests/rules.rs` |
| K5 | Gridlines are faint and horizontal only; no axis lines, no ticks | test `crates/tiles-engine/tests/rules.rs` |
| K6 | Categories follow one fixed order | test `crates/tiles-engine/tests/rules.rs` |

## Marks

| # | Rule | Checked by |
|---|---|---|
| M1 | Bars carry their values, so a bar chart has no y axis | test `crates/tiles-engine/tests/rules.rs` |
| M2 | Bars start at zero and have softened ends | test `crates/tiles-engine/tests/rules.rs` |
| M3 | No axis title repeats a field name, and x labels stay flat | test `crates/tiles-engine/tests/golden.rs` |
| M4 | Lines are firm with points; areas are light and start at zero | test `crates/tiles-engine/tests/rules.rs` |
| M5 | The plot has no frame | test `crates/tiles-engine/tests/golden.rs` |
| M6 | Hairlines and edges are crisp at every scale | test `crates/tiles-engine/tests/crisp.rs` |
| M7 | A long run of columns gets a light axis, and only the named values are written | test `crates/tiles-engine/tests/rules.rs` |
| M8 | Several lines are named at their ends, with a legend only when the names cannot fit | test `crates/tiles-engine/tests/rules.rs` |
| M9 | A note sits beside the mark it is about, clear of every other text and line, or is refused | test `crates/tiles-engine/tests/rules.rs` |
| M10 | The exception the title names leads | test `crates/tiles-engine/tests/rules.rs` |
| M11 | A bullet grades its target | test `crates/tiles-engine/tests/rules.rs` |
| M12 | Shares stack across one full bar, and a share that only rounds to 0 or 100 reads `<1` or `>99` (ADR-051) | test `crates/tiles-engine/tests/rules.rs` |
| M13 | A lollipop marks the average the intent sends, or the rows' mean when it has two or more rows, and no value label crosses it (ADR-051) | test `crates/tiles-engine/tests/rules.rs` |

## Motion

| # | Rule | Checked by |
|---|---|---|
| N1 | The card enters once and settles; dashboard cards start one after another | test `crates/tiles-engine/tests/rules.rs` · test `crates/tiles-engine/src/motion.rs` · test `crates/tiles-cli/src/wrap/paint/tests.rs` |
| N2 | Reduce motion is gentler, not zero | test `crates/tiles-engine/src/motion.rs` · test `crates/tiles-cli/src/motion.rs` |
