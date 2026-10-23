# Open in a window

On macOS `tiles open <id>`, typed in a terminal or as `!tiles open <id>` in a harness, shows a card in a window of its own: the same picture the terminal drew, a whole dashboard included, at its own pixel size, titled with the dashboard's or card's title. It prints `Card <id> in <theme>: <path>` and `Opened in a window of its own; Cmd-W closes it.`, and the window stays after the command ends. When the theme in force differs from the card's, the window shows the card drawn again in that theme at the size it was first drawn, under a new id. Every `tiles_chart` result on macOS names the command, and for a dashboard asks the model to tell the user (ADR-023).

## Sub-features

- `open-window` shows a card from the cache in a titled macOS window that keeps the card's shape and closes with Cmd-W.
- `open-window-dashboard` shows a dashboard as one picture with the same layout as in the terminal.
- `open-window-line` puts "The user can open this card in a window of its own by running `tiles open <id>`." in the chart result on macOS, with "Tell them so in a few words." for a dashboard and on the text route; on the text route under `tiles run` with the companion window on (`TILES_COMPANION=1`) the line is left out ([terminal routes](./terminal-routes.md), `route-companion-wording`).
- `open-window-off` keeps the window closed with `TILES_WINDOW=off` (or `0`, `false`, `no`), printing only the card line; the same values keep `tiles run`'s companion window closed ([terminal routes](./terminal-routes.md), `route-companion-window`).

## How to get to it (user POV)

- Ask for a chart or a mini dashboard in a harness, then type `tiles open <id>` in a terminal (or `!tiles open <id>` in OpenCode or Claude Code) with the id from the result or the model's reply.

## Driving it with drive.sh

Preconditions:

- macOS with a logged-in desktop; `scripts/doctor.sh` passes.
- A card in the cache: run `tiles card crates/tiles-engine/tests/intents/column.json` with `TILES_CACHE_DIR='$OUT/cache'` and take its `card` id.

- **Window.** Run `TILES_CACHE_DIR='$OUT/cache' tiles open <id>`. Exit 0 within about 2 s with the two lines above. A window titled with the card's title shows the card on the dark window background (light for the light theme). Find it with `osascript -l JavaScript` and `CGWindowListCopyWindowInfo` by its title, capture it with `screencapture -l <window id>`, and check the picture is the card's PNG. Cmd-W closes it and its `osascript` process ends (`pgrep -f "function run(argv)"` finds none).
- **Dashboard.** In a live OpenCode dashboard run (see [terminal routes](./terminal-routes.md)), run `tiles open <id>` with the dashboard's id and the session's `TILES_CACHE_DIR`. The window shows the heading, the scorecard row and the chart below it, laid out as in the terminal.
- **Result line.** `cargo test -p tiles-mcp a_card_for_a_window_is_its_own_picture_unless_the_theme_changed` checks the line and that a card in the theme in force is shown as itself.
- **Off.** Run the window command with `TILES_WINDOW=off`. Exit 0, one line, no window. `cargo test -p tiles-cli --test open` runs this way.

## Gotchas

- The window's app shows as `osascript` in Cmd-Tab and the Dock, with the card as its icon.
- The picture is the terminal's: enlarging the window past its size softens it.
- `tiles open` with `TILES_DRAW` set (the MCP server's environment) draws the card inline instead, and shows an Omni card as it was drawn with no window ([themes](./cli-theme.md) has the line it prints, ADR-027); a person's shell never has it set.
- Off macOS there is no window and the result leaves the line out.
