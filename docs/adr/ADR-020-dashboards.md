# ADR-020: `tiles_chart` draws a dashboard in the conversation's width, corrects a model's slips instead of refusing them, and plays its cards in one after another

- Status: Accepted; its 150 ms stagger and fixed frame count are superseded by [ADR-030](ADR-030-entrance-by-the-clock-on-every-route.md): 50 ms apart, at most 200 ms, frames 50 ms apart; its corrections are amended by [ADR-045](ADR-045-chart-call-keys-at-the-top-level.md), which reads a call nested under one `intent` key as the call and names the keys Tiles left out in a refusal
- Date: 2026-10-18
- Supersedes: [ADR-016](ADR-016-card-width-and-rows.md)'s shared width in a row and its refusal of a percent unit on a count; [ADR-017](ADR-017-early-card-and-entrance.md)'s entrance drawn at the card's resolution, sent with the card as one image, with every card of a row at once; [ADR-003](ADR-003-chart-intent.md)'s and [ADR-012](ADR-012-prototype-parity.md)'s refusals of a highlight that names no category and of a note with nowhere to go, for cards `tiles_chart` draws

## Context

The owner, after the row of scorecards worked: "goal now is to dynamically have several tiles appear like a dashboard. since one tile does fine now, lets aim for mini dashboards and dynamic layouts", and "For the mini dashboard, live in terminal with the option to open them in a separate window." A `row` drew at most 4 cards on one line, all as wide as the widest.

The first live dashboard in OpenCode (kitty, gpt-5, 2026-10-18) showed what else had to change. The owner: "render was slow, i could see it load slowly and then appear all at once, zero animation i think. also doesn't look like a dashboard? its all in one row, doesn't follow a nice dashboard layout", "its breaking the opencode main screen by the last tile going over the right side panel", and "this does this call to tiles_tiles_chart fail the first time? its red and works the second time?" In that run:

- The model put three scorecards and a line chart in one row, so each scorecard was stretched to the chart's height around one number.
- `tiles run` handed the server the terminal's 182 columns, and OpenCode's sidebar takes 42 of them.
- The wrapper wrote the 3.46 MB animation to kitty in one write, which held the harness's output back for 3.4 s.
- The model's first call was refused for highlighting the value 120000 instead of its month.

## Decision

### Layout

- `tiles_chart` takes three optional keys beside the call's own intent.
  - `row`: up to 3 more cards on the first row.
  - `rows`: up to 3 more rows of 1 to 4 intents.
  - `dashboard`: a `title` and an optional `note`, such as the data source or date range.
- A call draws at most 8 cards. Everything comes out as one picture and one placeholder block.
- `board::plan` lays the dashboard out.
  - Every scorecard goes in the top rows, in order, at most 4 to a row and as even as can be (five take three and two). The other cards of each asked row take a row of their own below.
  - A row that does not fit at its forms' narrowest widths splits over the fewest lines whose widths differ least.
  - The dashboard is as wide as its widest line asks for, within the room. Each line is fitted to that width, narrowed alike or widened alike. A chart whose range reaches 72 columns stretches; a scorecard stays at 35, and a slope or bullet stops at its own maximum.
  - The result text says when scorecards moved or a row split.
- For the live ask (three scorecards over a line chart of 12 months), the dashboard is 107 columns wide at 120 and 200 columns. At 80 columns the scorecards take two lines and the chart is 72 columns.
- Cards in a line share a height and line up part for part (ADR-016). No card among others draws its form's default subtitle, and a subtitle that repeats its title is dropped.
- The heading is drawn by the engine (`Engine::heading`): the title one step above a card title (18 px against 15), the note in the subtitle's style, text with no card box.

### The conversation's width

- `tiles run opencode` gives the server the conversation's width, not the terminal's. OpenCode 1.19.3's own TUI source sets this: the sidebar is `auto` while the terminal is wider than 120 columns, and the messages are `width-(sidebar?42:0)-4` wide. `Harness::side_panel` holds that rule.
- The wrapper writes the width on start and on every resize to `cols-<session>` in the cache. The server reads it on each call.
- The wrapper never draws past the conversation's right edge, nor past text the harness wrote right of the block's rows. A card wider than that shows its left part, cut at the edge.

### Speed and motion

- The server draws the entrance at half the card's resolution and ends it with the settled picture at that resolution. That is 0.78 MB for the live dashboard where the full resolution took 3.2 MB, and the call takes 0.41 s where it took 0.87 s.
- The cards enter one after another, row by row and left to right, 150 ms apart (at most 600 ms from first to last). The frame count stays the harness's (`TILES_FRAMES`); `frame_ms` grows to cover the longer entrance.
- The wrapper sends the crisp settled card the moment it is in the cache. It sends the entrance afterwards as an image of its own, at most 64 KB per 50 ms tick, so the harness's output is never held back.
- If the entrance has arrived when the block's first row shows, the entrance is placed over the block and runs. Otherwise the crisp card is placed, settled, and the entrance is dropped.
- Once the entrance has played and the block is whole, the entrance's image is deleted and the crisp card stays in the placeholder cells.
- A card placed early follows its block when the harness scrolls the block's first rows off the top before its last row is typed.

### Painting a consistent screen

- While the harness holds a synchronized update, the wrapper paints only when the update ends, just before the `?2026l` and inside the update, or after it has been held a second. Painting mid-frame had cut a card at text from the next frame and blanked a letter OpenCode had just written.
- When a block moves, the wrapper writes back what the harness last wrote in the cells its placeholders left. OpenCode redraws only the cells it changed, so pieces of the card had stayed above the block.
- `TILES_TRACE=<file>` makes the wrapper log each card's events with their times: in the cache, sent, entrance in the terminal, placed early, run, stopped, handed over, written back, and slow writes.

### Corrections instead of refusals

A card `tiles_chart` draws is refused only where drawing would show a wrong number or nothing at all:

- pre-scaled rows (ADR-016);
- an empty table, or more than 10,000 rows;
- a missing required role, or a field naming no column;
- an unknown form;
- negative values in a stacked form;
- too many series to tell apart;
- text with digits in a number column that is not a plain number, such as `48,600` or `$4.8M`: as a gap it could move a scorecard's headline to another period;
- control and bidi characters in text (ADR-009).

Everything else is corrected, and the result text says what changed in one sentence per correction. A sentence names only what the model wrote, never a value from the rows, so an Omni card's result keeps its values on this machine (ADR-013). `tiles render` prints the same sentences to stderr:

- Keys Tiles does not take are left out. Any version reads as 1, and a missing or unknown claim kind or tone takes the default.
- A unit written as a symbol or a word reads as that unit (`$` is usd). An unknown unit is left out, and a percent unit on a measure reaching 1,000 draws a count.
- A title, subtitle or note past its limit is cut at a word. A missing title is the measure's name.
- More than 50 highlight values keep the first 50. A highlight that names a measure's value highlights that value's row; one that names nothing on the card is left out.
- On a form without categories, a highlight is ignored. Of several series named where one can take the accent, the first keeps it.
- A role the form does not have is left out. Text in a number column that spells a plain number is that number, to the pre-scaled and percent rules as well, and text with no digit (`n/a`) becomes a gap.
- A field naming a column with other capitals or spaces names that column.
- A note with no highlighted mark, on a form that draws none, or with no room beside its mark, is left out.

The engine's own form rules still refuse these cases for every other caller.

## Consequences

- The tool description, the schema's descriptions and the session instruction ask for one dashboard call whenever several metrics are asked for. They also ask for scorecards together, 12 periods per scorecard, a short label per card, and the title and note once under `dashboard`.
- In the live check the block's first row came 1.5 to 1.6 s after the tool result: that is the model typing its reply. The crisp card was in kitty 30 to 90 ms after the result and the entrance 0.8 s after it, so the card showed in the same paint as the block's first row.
- A card in a terminal narrower than its block shows its left part. `OpenCode`'s sidebar toggled by hand is caught by the text on its rows, not by the width rule.
- Another harness with a side panel needs its own `Harness::side_panel` entry, read from its source.
- Opening a dashboard in its own window is the next step and needs a record of its own.
