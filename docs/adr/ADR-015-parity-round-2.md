# ADR-015: Five forms draw as their gallery cards, and the horizontal 100% bar is its own form

- Status: Accepted
- Date: 2026-10-18
- Supersedes: the big number's change line in [ADR-012](ADR-012-prototype-parity.md)

## Context

After parity round 1 (ADR-012), a review of the engine's cards beside the prototype's (`core/examples` 15, 33, 40, 41 and 44) found five forms still drawing something else. The bullet drew a light grey bar on one track where the gallery draws a dark bar on three bands that grade the target. The lollipop had no average rule. The dumbbell had a legend and a value beside every dot where the gallery has a minutes axis, one gridline per row, the periods named over one row's dots and its accent on every second dot. The big number added a change line and a 0 to 12 axis where the gallery writes only the number over its sparkline and "12 months ago" at its start. Gallery 44 is a horizontal 100% stacked bar with each share inside its segment, and the engine compared it against its column stack.

## Decision

- The bullet draws, per row, bands under 70%, 70 to 90% and 90 to 115% of the target, a thin bar in the accent or red when named and dark muted grey otherwise, an ink tick at the target, the share of target past the bands in the bar's colour, and a value axis (playbook M11).
- The lollipop marks an average with a dashed rule named above the rows, over a value axis (M13). The form gains an optional `average` role, since the average a title means (all payments) is not always the mean of the rows drawn; without it the rule stands at the rows' mean.
- The dumbbell draws a value axis with a gridline per row and no value beside its dots. Every second dot takes the accent; a bad finding's named rows are the exception, red on a pale red bar (`bad_soft` in the tokens); any other tone's named rows lead and the rest fade. The periods are named once, over the dots of the first row in the accent with room between them, or in a legend when no row has room (M10).
- The big number writes only its number and, behind it, the sparkline, which stops short of the card's edge and names where it starts: `12 months ago` for `YYYY-MM` rows a year apart, counted from the first period to the last, else the first period. An intent with a `target` adds the `N% of the X target` line and under it a 6 px rounded track filled by the share of target in the accent (red for a bad tone), with the sparkline below the bar; the gallery's big number has no target, so its look is unchanged. The change line ADR-012 described, with its `+3 pts` for a percent measure, is gone; the subtitle states the change, as the gallery's does.
- `stacked-100-bar` is a new form on a new `shares` layout (M12): horizontal bars stacked to 100%, shares of 10% or more written inside, a named series ranking the bars by its share. The column `stacked-bar` keeps its own drawing and has no gallery counterpart, so it leaves the reference test; gallery 44 is now compared with `stacked-100-bar`.
- A note's knockout no longer covers other text: a note place is refused when its knockout, 2 px past its letters, would touch another text.

## Consequences

- The five gallery cases sit much closer to the prototype (tests/reference.rs records each before and after): the bullet from 19.0 to 3.8 mean channel difference in the light theme, the share bar from 41.4 to 4.6, the big number from 6.7 to 2.9, and the lollipop, big number and share bar now match the prototype's height.
- The bullet and dumbbell stay 15 px shorter than the gallery's, which adds an axis title (`€ thousand`, `Minutes`); intent v1 has no axis title, and the subtitle names the unit.
- A big number no longer reports its change from the previous row on the card; a title or subtitle that needs it says it.
