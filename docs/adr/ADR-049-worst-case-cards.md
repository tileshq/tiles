# ADR-049: Every form has worst-case goldens, and no compared value reads as zero

- Status: Accepted; its glyph, joiner and right-to-left follow-ups are decided by [ADR-050](ADR-050-macos-system-fonts-behind-inter.md), and its title, label, lollipop and share follow-ups by [ADR-051](ADR-051-worst-case-layout-follow-ups.md)
- Date: 2026-10-21

## Context

The property tests in `crates/tiles-engine/tests/property.rs` prove that mutated intents draw or are refused without panicking, but nobody had looked at what a hard case draws. Break-ui's catalog (MIT) lists the cases a layout should survive: a title past the 44-character wrap, a 30-letter word (`Benachrichtigungseinstellungen`), CJK, Arabic and emoji labels, a ZWJ emoji sequence, one row, no rows, and values from -42.5 to 12,345,678.9. Its rule 58 says a number people compare is never cut short.

Drawn once per form, those cases showed three faults the engine could fix in place:

- A chart's compact step came from its largest value alone, so 1,284, 1 and -42.5 beside 1,284,000.5 all read `0.00M`, and the minus sign went with them.
- A category label wider than its band was centred on the band, so the 30-letter word under the first column ran off the card's left edge (column, waterfall, grouped and stacked bars).
- A slope chart set each left label as one run, name then value. With an Arabic name, bidi ordering put the value on the name's left.

Others need more than a local fix and are listed as follow-ups: Inter carries no CJK, Arabic or emoji glyphs and font fallback is off (playbook T1), so those labels draw as empty boxes; a ZWJ emoji sequence is refused in text a model sends ([ADR-009](ADR-009-security-baseline.md)); and `title_wrap_chars` is not read, so a title wraps only at the card's width.

## Decision

- **Worst cases are goldens.** `crates/tiles-engine/tests/intents/worst-case/` holds a `<form>-worst` intent for every form, built from the catalog's titles, labels and numbers, and a `<form>-one-row` intent with a title of exactly 44 characters. Thirteen forms' worst cases hold 1,284,000.5 and a negative value, between -1,284 and 12,345,678.9. Lollipop and multi-line are percent charts and hold -3% to 142%. Stacked-bar and stacked-100-bar hold 0 to 12,345,678.9, because both refuse a negative value; `stacked-bar-negative` and `stacked-100-bar-negative` each send -42.5 and record the refusal. `tests/golden.rs` draws each in the default theme against `worst-case/<case>-<theme>.png` in the build's golden set, or checks the one-line refusal it records for it. Every worst case sent with no rows is refused with `data.rows: is empty; send at least one row`. `tests/rules.rs` checks that every worst case that draws keeps its text on the card and its axis labels apart.
- **A value under a tenth of the chart's compact step takes its own step.** On a chart that writes millions, 1,284 reads `1,284`, 1 reads `1`, and -42.5 reads `−42.5`, beside `1.28M`. A headline's change already did this (`+$27K` beside `$1.03M`); the rule now covers every value label. A value of 0.1 step or more keeps the chart's step and its one decimal count.
- **An axis label stays on the card.** A category label that would cross the card's inner edge moves in until it fits, and a label that would then come within 8 px of the label before it is left out.
- **A right-to-left name is set apart from its value.** On a slope chart's left side, a name holding a Hebrew, Arabic, Syriac, Thaana or N'Ko letter is drawn as its own run ending where the value's run starts, so the value stays on its right. Left-to-right names keep the single run, and their pixels do not change.

## Consequences

- No existing golden changes: no fixture holds a value under a tenth of its chart's step, a label past the card's edge, or a right-to-left name.
- A chart can mix steps (`1.28M` beside `1,284`). Each value then reads at its true size, which a person comparing them needs more than one shared suffix.
- An owner can open the worst-case goldens to see the hardest cards Tiles draws today, empty boxes included, and a fix to glyph coverage or title wrapping shows up as a diff in them.
