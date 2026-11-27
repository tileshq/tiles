# ADR-066: Histogram, box plot and range bands draw from their gallery cards, and intent v1 takes an optional axis title

- Status: Accepted; amends [ADR-003](ADR-003-chart-intent.md)'s key table with `axis`, and settles what [ADR-015](ADR-015-parity-round-2.md) left open: the bullet and dumbbell axis titles intent v1 could not carry, and the bullet's `3k` ticks
- Date: 2026-11-26

## Context

Parity rounds 1 and 2 ([ADR-012](ADR-012-prototype-parity.md), [ADR-015](ADR-015-parity-round-2.md)) left eight gallery forms undrawn: histogram (gallery 18, 19), box plot (23), range bands (20), funnel (48), diverging bar (02, 05), bump chart (13), small multiples (29) and calendar heatmap (30). The owner asked for them on 26 November 2026, each with gallery references, goldens and rule tests, shipped a few forms to a pull request, and for three gaps the round-2 contact sheet showed: the bullet's `€ thousand` axis title and full `1,000` ticks, the lollipop's `Average €103` keeping its currency, and the dumbbell's `Minutes` axis title. ADR-015 recorded that intent v1 had no key for an axis title, so the bullet and dumbbell drew 15 px shorter than the gallery's.

## Decision

### An optional axis title

An intent takes an optional top-level `axis`, at most 40 characters, trimmed, cut at a word past the limit with a note, and refused for control or bidi characters as every text is (ADR-009). It is drawn centred under the value axis, in the label size at weight 500, 4 px under the tick labels, by the forms whose value axis runs along the bottom: lollipop, dumbbell, bullet, histogram, box plot and range bands. Their form summaries say "Takes an axis title". Any other form refuses it in one line (`axis: form column draws no value axis along the bottom; drop axis and name the unit in the subtitle`), and on the MCP route the correction pass leaves it out with a note, as it does a note a form cannot draw ([ADR-020](ADR-020-dashboards.md)). A free-text title was chosen over deriving one from `unit` or a field name: `€ thousand` and `Minutes` name what the numbers count, which neither the unit list nor a column name such as `Q2` says, and playbook M3 already forbids an axis title that repeats a field name.

### The three gaps

- **Bullet.** Its ticks are written in full (`1,000`, `2,000`), as the gallery's bullet writes them; other forms keep `3k` for whole-thousand ticks. `€ thousand` comes from `axis`.
- **Lollipop.** The engine already wrote the measure's unit on the average's name; the gallery intent had no `unit`. `tests/intents/gallery/lollipop.json` now sends `"unit": "eur"` and reads `Average €103`. Playbook T7 keeps the unit on every value and tick too (`€412`, `€100`), where the gallery wrote bare values; the unit on every number is the rule this engine was built to.
- **Dumbbell.** `Minutes` comes from `axis`.

### Three of the eight forms

This record covers the three distribution forms, which share the value axis the `axis` key titles; the other five follow in their own records. Each is a form file in `assets/forms/` on a new layout, so its roles, validation and `tiles forms` text come from one definition (ADR-003). Sizes the tokens do not carry are named constants in each layout, as the dumbbell's and bullet's are, since design pack 1 is pinned and a new token field would refuse it.

- **histogram** (`x` number, optional `y` and `threshold`): touching columns a hairline apart over a value axis, a count axis on the left with whole-number ticks for whole counts. Without `y`, Tiles counts the values into round bins (Sturges' count, 5 to 30); with `y`, `x` is each bin's start, each bin runs to the next start, and the last runs one narrowest gap past its start; the columns and tooltips show those ranges. A threshold draws a dashed ink rule, colours the bins from it on in the accent (red for a bad finding) and the rest grey, and writes `N% at T or more` beside the rule. Counted bins put an edge on the threshold, so the share is exact; a threshold past the first sent start is accepted when it equals a start or the last bin's end, and refused otherwise, since the share past it would be a guess. Counts below zero and more than 60 bins are refused. A histogram has no category, so `highlight` is refused.
- **box-plot** (`x`, `y`): one row per value; Tiles computes each group's quartiles as d3 and Vega interpolate them, whiskers to the furthest values within 1.5 boxes, and muted dots past them. The named group's box takes the accent and the rest are grey; the median is a line in the card's colour. A group with no spread draws a 4 px box with no median line.
- **range-bands** (`x`, `lo`, `hi`, optional `mid`, `inner_lo`, `inner_hi`): a pill from `lo` to `hi` at 30% of its colour, the inner pill at 70%, an ink dot at `mid`, each row on a faint row line. Grey rows take 55% and 100%, or a dark card all but hides them. One row per category (bounds cannot be summed), `lo` not above `hi`, `inner_lo`, `inner_hi` and `mid` between `lo` and `hi` so the bands nest, and both inner ends or neither.

## Consequences

- `tiles forms`, the MCP tool's form list and its schema grow by three forms and the `axis` key; no existing intent changes meaning.
- Every new form has light and dark goldens in `tests/golden-plain/`, a `-worst` and a `-one-row` worst case, rule tests M14 to M16 and the axis-title test (T9) in `tests/rules.rs`, a fixture the intent fuzz corpus seeds from, and a `verify-tiles` feature file. No existing golden changed: the bullet fixture's ticks were already written in full, and no fixture sent an axis title.
- The house goldens live in the design repository, whose `scripts/test-house.sh` runs this repository's tests at a pinned public tag; the new forms' Obsidian, light and dark house goldens join it when that tag moves past this change.
- Related: [ADR-003](ADR-003-chart-intent.md), [ADR-012](ADR-012-prototype-parity.md), [ADR-015](ADR-015-parity-round-2.md), [ADR-049](ADR-049-worst-case-cards.md).
