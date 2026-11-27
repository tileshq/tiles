# ADR-067: Funnel, diverging bar, bump chart and calendar heatmap draw from their gallery cards

- Status: Accepted
- Date: 2026-11-27

## Context

[ADR-066](ADR-066-distribution-forms-and-an-axis-title.md) drew three of the eight gallery forms parity rounds 1 and 2 left undrawn, the distribution forms that share a bottom value axis, and said the other five would follow in their own records. This record covers four of them: funnel (gallery 48), diverging bar (02, 05), bump chart (13) and calendar heatmap (30). Small multiples (29) follows in its own record. None of the four draws a value axis along the bottom, so none takes ADR-066's `axis` title.

## Decision

Each is a form file in `assets/forms/` on a new layout, so its roles, validation and `tiles forms` text come from one definition (ADR-003). Sizes the tokens do not carry are named constants in each layout, as ADR-066's are, since design pack 1 is pinned and a new token field would refuse it.

- **funnel** (`x`, `y`): steps in the order sent, each bar over a track-coloured ghost of the step before, written `8,200 (82%)` with its share of the step before; a share that only rounds to 0 or 100 reads `<1%` or `>99%`, as M12's shares do. No axis (M1). Counts below zero are refused, with a hint to send how many reached each step.
- **diverging-bar** (`x`, `y`): largest first, bars either side of an upright muted zero rule, each value with its sign past its bar's end. The scale runs from the lowest value to the highest with no nice ends, since no ticks are drawn.
- **bump** (`x`, `y`, `series`): Tiles ranks the series in each period, largest first, ties in series order, and draws a line through the ranks with `#1` at the top. The named line is firm with large points and the rest thin with small points in the ordered context greys (K1). The series still ranked in the last period are named in one column at the right edge, each beside its own lane; a line that drops out earlier, the highlight included, keeps its colour and tooltip and ends in a dot at its last point with no name, so the card's title carries its story. The right margin is sized for that column's names alone. Two periods at least. The ranks down the left stop at the most series any one period ranks, and a series with no number in any period is left off, so a top five that changes members over the year draws #1 to #5 and no rank sits beside an empty lane.
- **calendar-heatmap** (`x`, `row`, `y`): a grid of cells, each the track's shade with the accent over it from 20% at the lowest value to full at 80% of the way up, and ink laid over the top fifth, so the peaks a title names stand out on light and dark cards alike. A key under the grid runs from zero (or the lowest value) to the highest. A highlight names rows, columns or both; the cells where they meet write their values and the names they sit under are bold ink. Up to 31 rows. The gallery's sketch shades on a square-root scale; on the dark themes that left most cells blending into the card and the peaks barely darker than the rest, so the scale is linear and the top end takes ink.

A highlight names a funnel step, a diverging bar's category, a bump chart's series, or a heatmap's rows and columns; the pane's click cross-filter reads the same roles from `Layout::highlight_roles`.

## Consequences

- `tiles forms`, the MCP tool's form list and its schema grow by four forms; no existing intent changes meaning.
- Every new form has light and dark goldens in `tests/golden-plain/`, a `-worst` worst case and a `-one-row` case (bump refuses one row, since it ranks across periods), rule tests M17 to M20 in `tests/rules.rs`, a fixture the intent fuzz corpus seeds from, and a `verify-tiles` feature file. No existing golden changed.
- The house goldens live in the design repository, whose `scripts/test-house.sh` runs this repository's tests at a pinned public tag; the four forms' Obsidian, light and dark house goldens join it when that tag moves past this change.
- Related: [ADR-003](ADR-003-chart-intent.md), [ADR-015](ADR-015-parity-round-2.md), [ADR-049](ADR-049-worst-case-cards.md), [ADR-051](ADR-051-worst-case-layout-follow-ups.md), [ADR-066](ADR-066-distribution-forms-and-an-axis-title.md).
