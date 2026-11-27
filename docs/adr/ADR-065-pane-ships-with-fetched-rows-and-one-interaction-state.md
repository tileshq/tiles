# ADR-065: `tiles pane` ships in every build, draws the chart tool's calls with their fetched rows, and keeps what the user picks as one state: cross-filter, legend toggle, sort and an Omni timeframe drill

- Status: Accepted; the cross-filter, a click on a mark narrowing the other cards to its value, is superseded by [ADR-069](ADR-069-pane-polish-and-focus-on-click.md), where a click focuses its value and every card keeps its rows
- Date: 2026-11-26
- Amends: [ADR-063](ADR-063-interactive-pane-in-the-skia-engine.md), whose prototype had no `pane` command in a release build and refused any call naming a data source; [ADR-001](ADR-001-product-scope.md)'s interactivity non-goal, a second time

## Context

ADR-063 built `tiles pane` as a debug-only prototype and set the measurements that decide go or no-go. On 26 November 2026 the Ghostty run on the work laptop (Apple M3 Pro, Ghostty 1.4.0) passed every timed gate: hover p95 3.1 ms against 16, 12.9 KB a hover update against 64, one card after a click 8.2 ms against 16, the eight-card redraw 17.4 ms against 50, the first frame 49.4 ms against 200, 28.5 MB resident against 100 and 0.34% idle CPU against 1%. The table scroll step took 56 ms against 33, a miss ADR-063 assigns to the table form, and the hover as seen in a 60 fps recording is still to be measured. The same day the owner answered the offer to start stage 1, "the shippable pane. That means real data from Omni and the agent opening the pane beside itself", with "yes start both".

ADR-063's first step is the pane, shippable: interaction state in the scene, cross-filter, legend toggle, sort and drill, the MCP tools that open the pane and read what the user picked, and split openers per terminal. It goes in as several pull requests. This record covers the first: the pane in release builds, drawing real cards with their rows, and the interaction state. The MCP tools and the split openers get their own record with their pull request.

## Decision

### The command

`tiles pane <intent.json | card id> [--trace <file>]` is in every build and in the usage text. The argument is a chart call as `tiles_chart` takes it, one card or a dashboard, read from a file; or, when no file of that name exists and it is six hex digits, a card the chart tool drew, whose call Tiles reads from `<id>.call.json`. `-` is refused, since standard input is the keyboard. The table stress scene (`"stress": "table"`) and `TILES_PANE_TEST_PANIC` stay in builds with debug assertions; the `pane` profile stays for measuring.

### Rows

The pane reads a call as the chart tool does (`tiles_mcp::card::Call::read`) and fetches each card's rows through the same source seam as `tiles card` (`serve::Fetcher`): an Omni topic with the user's own configuration (ADR-008), a warehouse query from the first configured source that runs one (ADR-031). The rows stay in the pane's memory (ADR-027). The pane writes no file but the trace the user names, which holds timings, byte counts and card numbers, never a value. A saved call holds no fetched row, so `tiles pane <id>` for a card drawn from Omni asks Omni again, as asking for the chart again would.

### One interaction state

What the user picks is one value, `pane::state::State`: the cross-filter, the series hidden on each card, each card's sort, and each card's drill levels. Every card's intent comes from `State::apply` over the call's intents, in that order: the deepest drill, hidden series left out, the cross-filter, the sort. After any change the pane lays out again only the cards whose intent changed, in their own slots, and sends only those whose scene changed, as ADR-063's click does; the tooltip then follows the mark under the pointer in the new scenes. A card that refuses what the state asks keeps its picture and is named on the status line.

- **Cross-filter.** A click on a mark, as ADR-063 has it.
- **Legend toggle.** A click on a legend entry hides that series on that card only. `Engine::legend` gives each legend label's box, widened to the swatch drawn before it; a line chart's direct labels count. A label cut with an ellipsis finds the one series its text begins. The last series a card shows cannot be hidden. A hidden series leaves the legend, so `r` brings it back.
- **Sort.** `s` steps the card under the pointer through largest first, smallest first and its own order, by each category's total, so a category's series move together; a step that would leave the order as it is is skipped. It applies to the forms that draw their rows in the order they come: column, dot plot, dumbbell, grouped bar, stacked bar and 100% stacked bar. A bar chart and a lollipop already rank largest first, and a line, area, slope or waterfall keeps time order; for those the status line says the card keeps its own order.
- **Drill.** `d` on a mark of a card drawn from an Omni date field with a timeframe asks Omni again, through the source seam, for the same topic with the next finer timeframe (year, quarter, month, week, date) beside the drilled one. It sorts by the finer field where the query sorted by the coarser one, takes the default row limit, and keeps only the rows inside the drilled period, filtered in memory as they arrive. The drilled card drops its highlight and note, which name coarser periods, and its subtitle reads `<period> by <timeframe>`. `u` takes the card under the pointer back up a level. A card drawn from rows the call holds, or from a warehouse query, says on the status line what a drill needs and asks no source.
- **Keys.** `s`, `d`, `u`, `r` clears everything picked, `q` quits. The status line names the filter, the drill depth, how many series are hidden and how many cards are sorted.

The drill adds no Omni filter: ADR-008's "the topic's own default filters and no others" stands, and the intent schema does not change. A finer timeframe beside the coarser one, narrowed in memory, is the drill the existing seam answers without sending Omni a filter no fixture or document here has checked, or rewriting the SQL an AI wrote for a warehouse. A drill on a category, or on a warehouse card, waits for a record of its own.

### What stays

Inline cards, the placeholder block, the wrapper and every golden are unchanged: `Engine::legend` reads a scene and draws nothing. No MCP tool, add-on or `tiles run` path reaches the pane yet. The pane opens no port, writes no web page, and reaches the network only through the configured Omni and warehouse sources, the same hosts `tiles card` reaches. Every byte it writes still comes from `tiles_term::pane` and `tiles_term::kitty`, the status line through the one sanitizer, and every byte it reads goes through `tiles_term::input::Parser` and its fuzz target (ADR-063).

## Consequences

- ADR-063's "A release build has no `pane` command" and its refusal of a call that names a data source no longer hold; its measurements and its no-go rule still do.
- A drill holds the pane while Omni answers, within Omni's 60 s budget; the status line says what it is waiting for.
- A drilled card fetches up to the default 5,000 rows for every period and keeps only the drilled one's. A query whose finer rows pass 5,000 across all periods is cut, as any Omni card is.
- `tiles pane <id>` for an Omni card needs Omni configured in the shell that runs it.
- Tests: `crates/tiles-cli/src/pane/state.rs` for the state, the sort, the legend mapping and the drill's request; `a_drill_asks_omni_for_the_next_timeframe_and_keeps_only_its_month` in `crates/tiles-cli/src/pane/mod.rs` drives a drill through the replay server and its recorded requests (`fixtures/omni/query-orders-by-month*.json`); `crates/tiles-cli/tests/pane.rs` drives the legend toggle, the sort, a refused drill, a card id and a missing one through a pseudo-terminal.
- Related: [ADR-008](ADR-008-data-sources.md), [ADR-009](ADR-009-security-baseline.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-027](ADR-027-omni-rows-stay-in-memory.md), [ADR-031](ADR-031-warehouse-sources.md).
