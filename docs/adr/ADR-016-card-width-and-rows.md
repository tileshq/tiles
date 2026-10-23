# ADR-016: Cards size to their content, and a row of cards is one picture

- Status: Accepted; its rule that the cards of a row share one width and its refusal of a percent unit on a count are superseded by [ADR-020](ADR-020-dashboards.md)
- Date: 2026-10-18

## Context

Every card was the terminal's width less 8 columns, capped at 72, the prototype's rule. In OpenCode in a wide kitty window a big-number scorecard came out 72x23 cells for one number and a sparkline. Asked for two scorecards side by side, the model called `tiles_chart` twice and wrote the two blocks one after the other, so the cards always stacked (the KPI demo's report, problems 2 and 3). Three ways to get a row were weighed: the model pads and merges two blocks itself, which models do badly and harness fences may wrap; the wrapper pulls a later block up beside an earlier one, which rewrites the harness's own layout and scrollback; or the tool draws the row.

## Decision

- Each form file states a `width` range in columns: `min`, `max`, and optional `per_point` (columns per distinct `x` value) and `per_label_char` (columns per character of the longest `x` label). A card asks for `min` plus those steps, within its range; `Look::card_cols` fits that to the terminal less 8 columns and to 30 to 72 columns, and never below the engine's narrowest tile. A big number with 12 months takes 41 columns; a six-week column chart 61.
- A lone big number is the gallery card of ADR-015 at any width: its sparkline keeps the gallery's 130 px chart, so a narrower lone card is not shorter. Only a scorecard in a row is shorter (below).
- `tiles_chart` takes `row`: up to three more intents drawn to the right of the call's own intent, which is always the first card; an empty or null `row` draws that intent alone. The server lays them out as one picture with one placeholder block, so the wrapper, the add-ons and the model's copying work as for one card. Each item's schema is the generated intent schema without its descriptions; the keys and limits stay those of `tiles-intent` (ADR-003).
- The cards of a row share one width, the widest any of them asks for, narrowed to the room but never under any card's form `min`; otherwise the cards stack one above the other in the same picture and the result text says how many columns the row needed. Each row is laid out three times: alone, then with the row's most title and subtitle lines, then as tall as the tallest card (`Engine::scene_aligned`, `Align`), so titles, big numbers, change lines and sparklines sit on the same lines. `board::place` puts rows of drawn cards on the picture, and the engine draws them in one raster (`Engine::board_png`), each card exactly as it draws alone.
- A scorecard among other cards is compact, asked for by the owner after the first live row ("size wise these do seem big and they're both not the same size either"): its form's narrowest width (35 columns, the engine's narrowest tile at 16 px cells and scale 2, so two sit side by side in an 80-column terminal), no default subtitle, a 26 px sparkline with no period labels under it, about half a lone card's height (12 rows of 16 px cells against 24 for the first live row). The tool description and the session instruction ask for a short metric label as each row card's title and no subtitle, with the headline and any shared note written once in the reply. A lone big number keeps its normal title and layout.
- A headline and its change are written to three significant figures with no trailing zeros (23.6K, +0.7K, 5M), on the step the rounded value reaches (999,600 reads 1M, not 1,000K), and a subtitle never wraps so that a separator (`·`) starts a line.
- Rows given in thousands are refused when the title writes the headline in full: a K, M or B number in the title that reads as the measure's last value on the title's own step (within 0.8 to 1.25), while no number in the title matches a row as written. "$128K" over a last row of 128 is refused; "NPS among 50K customers held at 48" and "Signups, trailing 12M" over 12,000 a month draw. A percent unit on a measure that reaches 1,000 is refused too. Neither refusal names a row's value, since the rows may be Omni's (ADR-013).

## Consequences

- A later dashboard of several rows needs only a packer that returns more rows from `board::plan`; placement, alignment and drawing already take rows. Mixed widths within one row would need the shared-width rule relaxed for rows of unlike forms.
- A row of unlike forms stacks sooner than before: a big number beside a line chart takes the line chart's width, so both must fit at its `min`.
- A form file without `width` gets 40 to 72 columns, the old width for most terminals.
- A row's picture is wider than any one card (up to 4 x 72 columns plus gaps). The room is the terminal's width less 8, which assumes the harness gives its conversation most of the terminal's width.
