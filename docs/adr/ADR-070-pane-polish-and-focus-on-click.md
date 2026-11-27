# ADR-070: The pane gets the house polish: every line on the same edges, a page of the theme's colour, a coloured status line, a raised tooltip, and a click that focuses its value instead of filtering

- Status: Accepted
- Date: 2026-11-27
- Amends: [ADR-065](ADR-065-pane-ships-with-fetched-rows-and-one-interaction-state.md), whose click on a mark filtered the other cards' rows

## Context

After the Apple Silicon gate run on 26 November 2026 the owner said of the pane: "cool. the pane wasn't fancy and didn't have our design polish though", and later "in the pane tiles the top 3 scorecards aren't aligned to page width i think". The scorecards stop short because the dashboard planner caps a scorecard among other cards at its form's narrowest width (`Painter::fit`) and fits each line only to the widest line's asked width. On a 189-column window the three scorecards covered 107 columns under chart lines of 181. Around the cards the terminal's own background showed, the status line was plain text, and the hover drew a square white outline and a tooltip that could cover the card's title or the bar's value label.

The filter from ADR-065 had a second problem the mockups showed. Filtering the other cards to one region leaves a column chart with one giant column, a lollipop with one dot and a stacked bar with one stack, under titles such as "Germany places the most orders" that no longer describe what they draw.

On 27 November 2026 the owner saw static mockups of the pane: full width, half width, a hover, a click that focuses, and today's filter for comparison. The owner chose the focus option as drawn ("panes look good").

## Decision

### Layout

The pane lays its cards out with `Call::lay_out_pane`. It plans the rows as the chart tool does (`board::plan_full`), but over the window's width less `PANE_MARGIN`, two columns each side. It fits every line to that whole width: scorecards and slope charts stretch like the other charts, so every line shares the same outer edges and gutters at any width. Three scorecards that don't fit side by side split two and one as before, and each line still spans the width. Inline dashboards keep `board::plan` and every golden stays as it was.

### Page and heading

One pixel of the theme's page colour, placed with `kitty::fill` stretched over the whole window at stacking order -1,073,741,825, sits under the text, under cells with a background colour such as the status line's key caps, and under the cards. At -1 the key caps' backgrounds vanished under it in Ghostty, as the kitty protocol has it. The pane sends it only with the first frame and after a resize. The colour is the theme's new `page` token (plain dark `#171614`, plain light `#f1eee7`). A design pack without one takes the card colour a third of the way to black on a dark theme and a twentieth on a light one, so design pack 1 still loads. The heading is `Engine::pane_heading`, the card heading on the cards' left edge. With a value focused, a chip reads `column value ×`, then the `r` key that clears it. Where the title keeps at least 60% of the heading's width beside the widest chip (the column cut to a twelfth of the width, the value to a seventh), the chip sits at the right of the title line and the title wraps short of it. On a narrower pane the chip takes its own line under the title, and the title keeps the full width. The heading leaves the chip's room from the first frame whether a value is focused or not, so focusing one never moves the cards or rewraps the title. The note keeps the full width.

### Status line

`tiles_term::pane::status_spans` builds the status line from coloured runs, each through `sanitize::run`. On the left it shows `tiles pane`, the call's name, then either what the user picked (the focused value, how many cards focus it, drills, hidden series, sorts) or a hint, then any note. The keys `q r s d u` sit at the right as key caps. The keys drop when the line is too narrow for both. The last column stays empty so the line never wraps. The only control sequences are the cursor moves, the erase and each run's own colour, reset after it, which the hostile corpus test checks.

### Hover

The overlay outlines the mark with a two-pixel ink ring with rounded corners inside a card-coloured halo. A raised tooltip (`Palette::raised`, a hairline, a soft shadow) shows a swatch in the mark's colour and the mark's name, a rule, each other `name: value` line as a muted name and a value on the right, and a hint for the click. The hint reads "click to focus France", or "click again to clear" on the focused value. The tooltip never covers the card's title or any value label, and `pane::place_tip` tries three places in order. First it goes below the mark, starting just left of the pointer and sliding along the row until it is clear. Next it goes beside the mark, on the side with more room, within the plot's width, moving up or down until it is clear, from the plot's top down to the category labels under the plot, which it may cover. A tall column at full width lands here, beside its column and under the next column's value label; at half width, where the shorter columns' value labels leave no clear band inside the plot, it sits lower, over the category labels. Last it goes inside the mark's upper part, when the mark is wider than the tooltip. When no place is clear, as on a scorecard's sparkline, it falls back to below the mark, or above it when the tile has no room below, inside the tile. The ring and the tooltip go as two tight pictures (`pane::Overlay`'s `ring` and `tip`, image ids 6999990 or 6999991 and two more), so the empty space between a wide bar and a tooltip under it is neither drawn nor sent. No card is sent again on hover.

### Click

A click on a mark focuses its value (`State::focus`): every card keeps all its rows, and each card whose form highlights by the selected column and holds the value highlights it in the accent, whatever the card's claim tone. The form-aware check from PR #15 decides which cards can, so a stacked bar, which highlights only by its series, keeps its own colours when a region is clicked. A second click on the focused value, on whichever card holds it, a click on empty space, or `r` clears the focus. The pane still lays out and sends only the cards whose scene changed, and the heading when its chip changes.

## Consequences

- A click no longer narrows the other cards. A user who wants one region's rows alone asks the agent for that chart.
- A focus redraws the cards whose highlight changes and the heading. On the fixture a click on Germany sends two pictures (the lollipop and the heading) where the filter sent four.
- The pane's colours come from the theme tokens, so a release build draws the house theme and a build from source draws the plain one. The design pack takes the `page` token when its repository next changes.
- The tooltip carries more than before: a swatch, a rule, a hint and a shadow. Drawn as one picture with the ring, its hover took about three times as long on the 2017 Intel Mac and sent twice the bytes, which is why the ring and the tooltip go as two pictures. The gate numbers in the pull request show the result.
- Tests: `on_the_pane_every_line_spans_the_room` in `crates/tiles-mcp/src/board.rs`; `a_selection_focuses_its_value_on_every_card_that_holds_it` in `crates/tiles-cli/src/pane/board.rs`; the coloured status line in `crates/tiles-term/src/pane.rs` and `crates/tiles-term/tests/hostile.rs`; the page, the click's two pictures and the tooltip after a click in `crates/tiles-cli/tests/pane.rs`; the tall column's tooltip clear of the title and value labels in `a_tall_columns_tooltip_leaves_the_title_and_value_labels_clear` in `crates/tiles-engine/tests/rules.rs`.
- Related: [ADR-009](ADR-009-security-baseline.md), [ADR-021](ADR-021-obsidian-default-and-theme-switch.md), [ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md), [ADR-037](ADR-037-plain-theme-for-builds-from-source.md), [ADR-063](ADR-063-interactive-pane-in-the-skia-engine.md).
