# ADR-068: Small multiples draw from their gallery card

- Status: Accepted
- Date: 2026-11-27

## Context

[ADR-066](ADR-066-distribution-forms-and-an-axis-title.md) and [ADR-067](ADR-067-funnel-diverging-bump-and-heatmap-forms.md) drew seven of the eight gallery forms parity rounds 1 and 2 left undrawn. ADR-067 said small multiples (gallery 29) would follow in its own record. It is the last of the eight. Every other line form puts its series on one plot and keeps one accent, so a reader can follow one line through a tangle; small multiples give each series its own plot instead, which changes how a highlight and the series count are checked.

## Decision

**small-multiples** (`x`, `y`, `series`) is a form file in `assets/forms/` on a new layout, so its roles, validation and `tiles forms` text come from one definition (ADR-003). Sizes the tokens do not carry are named constants in the layout, as the other new forms' are, since design pack 1 is pinned and a new token field would refuse it.

- A panel per series, as many to a row as fit and fewer while that adds no row, on one shared scale, so the panels compare by shape and size alike.
- Each panel draws its line over the other series in faint grey, with its last value at its end.
- Every named panel takes the accent, or red for a bad finding, and the rest are muted, as K1 asks. Several named panels are allowed, since each has its own plot, so the check that a shared plot can tell its named series apart, and the correction pass that trims such a highlight to one series, do not apply to this form.
- Up to 12 panels, refused past that, and two periods at least, as the bump chart and the line forms need.

A highlight names a panel's series; the pane's click cross-filter reads the same role from `Layout::highlight_roles`.

## Consequences

- `tiles forms`, the MCP tool's form list and its schema grow by one form, to 25; no existing intent changes meaning.
- The form has light and dark goldens in `tests/golden-plain/`, a `-worst` worst case and a `-one-row` case that is refused, rule test M21 in `tests/rules.rs`, a fixture the intent fuzz corpus seeds from, and a `verify-tiles` feature file. No existing golden changed.
- The house goldens live in the design repository, whose `scripts/test-house.sh` runs this repository's tests at a pinned public tag; the form's Obsidian, light and dark house goldens join it when that tag moves past this change.
- Related: [ADR-003](ADR-003-chart-intent.md), [ADR-049](ADR-049-worst-case-cards.md), [ADR-066](ADR-066-distribution-forms-and-an-axis-title.md), [ADR-067](ADR-067-funnel-diverging-bump-and-heatmap-forms.md).
