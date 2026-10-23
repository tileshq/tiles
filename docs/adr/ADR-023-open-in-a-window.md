# ADR-023: `tiles open` shows a card in a window of its own on macOS

- Status: Accepted; supersedes how [ADR-021](ADR-021-obsidian-default-and-theme-switch.md)'s `tiles open` draws a card in a plain shell; [ADR-027](ADR-027-omni-rows-stay-in-memory.md) supersedes redrawing a card drawn from Omni rows, which `tiles open` now shows as it was drawn; its `TILES_WINDOW=off` is amended by [ADR-046](ADR-046-companion-window-on-the-text-route.md), which also keeps the window closed for `0`, `false` or `no` and reuses `window.js` for `tiles run`'s companion window on the text route, where the chart result leaves out the `tiles open <id>` line while that window shows the card
- Date: 2026-10-18

## Context

The owner, 18 Oct 2026, on the mini dashboards: "live in terminal with the option to open them in a separate window." ADR-004 planned a native window for terminals without images: the prototype's `tilefloat`, a Swift helper that put the card PNG in an `NSImageView` on a borderless panel glued to the rows a reply reserved. ADR-011 left it out of v0.1 until the companion window. ADR-021 then added `tiles open <id>`: inside `tiles run` it draws an earlier card again in the theme in force and prints the chart tool's text; in a plain shell it draws it again at the shell's own geometry and prints its PNG path. A plain shell has no `TILES_COLS`, so a dashboard drawn 138 columns wide in OpenCode came back 100 columns wide and could pack differently.

`TILES_DRAW` is set only in the MCP server's environment, so `tiles open` typed in a terminal, or as `!tiles open` in OpenCode or Claude Code, takes the plain-shell path.

## Decision

- **In a plain shell on macOS, `tiles open <id>` also shows the card in a window of its own.** It prints `Card <id> in <theme>: <path>`, then `Opened in a window of its own; Cmd-W closes it.` Inside `tiles run` it is unchanged.
- **The window shows the card as it was drawn.** When the card was drawn in the theme in force, the window shows its own PNG and no card is drawn. Otherwise its saved call is drawn again in the theme in force at the geometry it was first drawn at (the terminal width and cell size saved beside it), from the rows fetched the first time, and saved as a new card. A dashboard keeps its layout. This replaces ADR-021's plain-shell redraw at the shell's geometry.
- **The window comes from the system's `/usr/bin/osascript`.** It runs `crates/tiles-cli/src/window.js` as JavaScript for Automation: the prototype's `NSImageView`, in a titled window, with no Swift toolchain, no compiled helper and nothing installed. The window opens at the card's pixel size on the main screen, within 90% of the room there. It keeps the card's shape when resized and uses the light window appearance for the light theme and the dark one otherwise. The card is the Dock icon, and Cmd-W closes the window and ends the process. It runs in its own process group, so it stays after the command or the terminal ends. The PNG path and the title go to `osascript` as arguments, never into the script, and the title passes the sanitizer first (ADR-009).
- **The chart result names the command.** On macOS every `tiles_chart` result says "The user can open this card in a window of its own by running `tiles open <id>`." For a dashboard of two or more cards, and on the text route, it also asks the model to tell the user in a few words.
- `TILES_WINDOW=off` keeps the window closed, for scripts and tests. Off macOS there is no window, and the result leaves the line out.

## Consequences

- The window shows a picture at the terminal's pixel density: no values on hover, no live data, and it softens when enlarged past its own size.
- Cmd-Tab and the Dock name the window's app `osascript`.
- A card drawn before ADR-021 has no saved call. It opens only when it was drawn in the theme in force.
- The window reads the PNG from the owner-only card cache and sends nothing anywhere (ADR-019).
