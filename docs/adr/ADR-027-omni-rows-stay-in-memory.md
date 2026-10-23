# ADR-027: Rows fetched from Omni never reach the disk; only the session that fetched them redraws their card

- Status: Accepted; supersedes the clause of [ADR-021](ADR-021-obsidian-default-and-theme-switch.md) that saves the rows fetched from Omni in `<id>.call.json`, and with it [ADR-023](ADR-023-open-in-a-window.md)'s redraw of an Omni card "from the rows fetched the first time"
- Date: 2026-10-19

## Context

A security review of Tiles on 2026-10-19 rated finding S1 high: "Omni values stay on this machine" held for the tool result and not for the machine. ADR-021 saved every card's call beside its picture as `<id>.call.json`, the rows fetched from Omni included, in plain JSON, so a theme change never asked Omni twice. Every harness gives the model a shell, Claude Code's Read tool shows a file to the model, and a user may run Claude Code with every permission prompt turned off. A model steered by a README, an issue or a data label could read those rows, and they would reach the provider. ADR-013 stops the result channel only. The owner asked for every finding resolved: "We are known for our product craft and polished and smooth products + novel engineering and design principles."

## Decision

- **The call file holds no Omni row.** For a card drawn from Omni, `<id>.call.json` carries the tool's arguments (the topic, fields, sorts and limit the model wrote, never a fetched value), an `omni` list marking which cards of the call came from Omni, and the geometry. A card drawn from rows the model sent is saved as before; those rows are already in its call.
- **The process that fetched the rows holds them in memory.** `Cache` keeps each Omni card's rows by id in the memory of the process that drew it, at most the 300 cards the cache keeps, and drops a card's rows when the card is pruned. Its debug print shows a count, never a row.
- **A theme change inside a session still swaps Omni cards in place.** The MCP server that drew the cards also runs `tiles_theme`, so `/theme` in a `tiles run` kitty session redraws them from its memory under the same ids, as ADR-021 describes. Any other process, `tiles theme` in a shell or Pi's `/theme`, which runs the binary afresh, leaves an Omni card in its theme and says so in one line: "The 1 card drawn from Omni rows keeps its theme, since Tiles keeps those rows only in the memory of the session that fetched them."
- **`tiles open <id>` shows an Omni card as it was drawn.** It runs as its own process, so for an Omni card it prints the card's own id, theme and PNG path, opens that picture in the window on macOS, and adds one line: "Card <id> was drawn from Omni rows, which Tiles keeps only in the memory of the session that fetched them, so only that session can draw it again in another theme; ask for the chart again to see it in <theme>." It draws no new card, inside `tiles run` or out. Inside a `tiles run` session, where `TILES_DRAW` is set, it prints the same lines and opens no window: a model that runs `tiles open` on the user's behalf shows nothing on their desktop.
- **A call file written before this record counts as Omni-backed** when it holds fetched rows, so it gets the same answer. Tiles never reads rows back from a call file.

## Consequences

- An Omni card from an earlier session, or from Pi, where each card is drawn by a `tiles card` process that exits, keeps the theme it was drawn in. Asking for the chart again draws it in the new theme, and that asks Omni again.
- The PNG still shows the values, and call files written before this change still hold their rows until the card is pruned; SECURITY.md and `docs/omni.md` tell an upgrading user to run `tiles cache clear` once. Keeping the model from reading the cache directory is the harness's job: the second part of S1 adds a read deny to `tiles run` and `tiles cache clear`, recorded where it lands.
- The rows of up to 300 cards can sit in the MCP server's memory for a session. They leave it when the harness ends the server.
- Tests: `an_omni_card_leaves_no_row_on_disk_and_opens_as_drawn_after_its_session` in `crates/tiles-cli/tests/open.rs` draws a card from planted replay rows and finds none of them in any file Tiles wrote, then checks the `tiles open` line; `an_omni_card_switches_theme_from_the_servers_memory_only`, `a_mixed_dashboard_redraws_its_omni_card_from_memory` and `the_hostile_corpus_in_a_saved_omni_card_reaches_tiles_open_plain` in `crates/tiles-mcp/src/server/tests.rs` cover the swap, a dashboard whose typed card precedes its Omni card, and the hostile corpus.
- Related: [ADR-008](ADR-008-data-sources.md), [ADR-009](ADR-009-security-baseline.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-021](ADR-021-obsidian-default-and-theme-switch.md), [ADR-023](ADR-023-open-in-a-window.md).
