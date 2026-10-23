# Themes and /theme

The themes are the list in the design tokens the build sealed, `themes.list` in `tokens.json` or a design pack's (ADR-036), and cards are drawn in the tokens' `themes.default` unless the user picks another. The house tokens carry Obsidian, a dark graphite theme, as the default, then light and dark; a build from source carries the plain theme's dark, its default, and light ([the plain theme](./plain-theme.md), ADR-037). The steps below expect the house build. `tiles theme` lists the themes with the one in force marked; `tiles theme <name>` saves one in `~/.config/tiles/config.json` (or `$TILES_CONFIG_DIR/config.json`), and the next card is drawn in it. Inside the harnesses the same choice is the `tiles_theme` MCP tool, reached as `/mcp__tiles__theme` or `/tiles:theme` in Claude Code, `/theme` in OpenCode, and `$tiles-theme` in Codex after `tiles install codex`. Pi's `/theme` runs `tiles theme` itself through the Pi extension. Inside a `tiles run` kitty session, cards already drawn switch in place; elsewhere only new cards change and `tiles open <id>` shows an earlier card in the new theme (ADR-021). A card drawn from Omni rows is redrawn only by the MCP server that fetched them, from its memory; any other process shows it as drawn and says why (ADR-027).

## Sub-features

- `theme-list` lists the build's themes in the tokens' order (obsidian, light and dark with the house tokens), each with the tokens' few words on its look, marks the one in force and says where it came from.
- `theme-set` saves a theme owner-only and the next `tiles render` without `--theme` uses it.
- `theme-env` shows `TILES_THEME` winning over the saved theme for a session.
- `theme-refused` refuses an unknown name in one line naming the themes.
- `theme-mcp` lists and sets through the `tiles_theme` tool and offers the `theme` prompt.
- `theme-swap` switches a card already on screen in kitty under the same image id, with no new placement.
- `theme-pi` lists and sets through Pi's `/theme` command.
- `theme-open` shows an earlier card in the theme in force with `tiles open <id>`, drawn again only when it was drawn in another theme ([open in a window](./cli-open-window.md) covers the window).
- `theme-omni` keeps an Omni card's rows out of `<id>.call.json`, swaps it in place from the MCP server's memory, and shows it as drawn, with one line saying why, from any other process.

## How to get to it (user POV)

- Run `tiles theme` to see the themes and `tiles theme light` to pick one.
- In Claude Code type `/mcp__tiles__theme light`; in OpenCode or Pi `/theme light`; in Codex `$tiles-theme light`.
- Run `tiles open <id>` with a card id from a chart result to see that card in the current theme.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.
- Point `TILES_CONFIG_DIR` at a fresh directory under the run so the user's own saved theme is untouched, and unset `TILES_THEME`.

- **List.** Run `TILES_CONFIG_DIR='$OUT/config' .agents/skills/verify-tiles/scripts/drive.sh <run> theme-list theme`. Exit 0; the first line starts `* obsidian` and the last says `Cards are drawn in obsidian (nothing chosen yet, so the default)`.
- **Set.** Run the same with name `theme-set` and `theme light`. Exit 0 and `Theme set to light, saved in .../config/config.json. The next card is drawn in it. Cards already on screen keep their theme in this terminal; tiles open <id> shows an earlier card in light.` `config.json` is mode 600 and holds `{"theme":"light"}`. Rendering the bar fixture with the same `TILES_CONFIG_DIR` and no `--theme` prints `light theme`.
- **Session variable.** Run `theme` again with `TILES_THEME=dark` added. The `*` is on dark and the last line says `TILES_THEME sets it for this session`.
- **Unknown name.** Run `theme neon`. Exit 1 and `tiles: "neon" is not a theme; use obsidian, light, dark`.
- **MCP.** Drive `tiles mcp` as in [the MCP server](./mcp-server.md) and call `tools/call` with `{"name": "tiles_theme", "arguments": {"name": "dark"}}`. The result text starts `Theme set to dark`. `prompts/list` offers `theme` with an optional `name`, and `prompts/get` for it asks the model to call `tiles_theme`.
- **Swap in kitty.** `cargo test -p tiles-cli --test pty kitty_swaps_a_card_in_place_when_the_theme_changes` drives `tiles run --terminal kitty codex` with a fake harness that draws a card and runs `tiles theme` with a theme other than the build's default. In a real kitty, run `tiles run claude`, ask for a chart, then type `/mcp__tiles__theme light`: the card already in the conversation turns white in place, scrolled-back copies too, with no second copy.
- **Pi.** `node --test adapters/pi/index.test.ts` drives `/theme` against a fake `tiles`. In a real Pi, run `tiles run pi` and type `/theme`: the list appears as a status line, with no model turn. `/theme light` prints `Theme set to light`, and `/theme neon` shows the refusal as an error.
- **Open.** After a chart in a session, change the theme and run `TILES_WINDOW=off tiles open <id>` with its id in a plain shell. It prints `Card <new id> in <theme>: <path>.png`, and the PNG is drawn in the current theme at the size the card was first drawn. Run it again with the new id: it prints that same id, since the card is already in the theme. Inside `tiles run` it prints the chart tool's text instead, a placeholder block where the wrapper draws.
- **Omni card.** `cargo test -p tiles-cli --test open an_omni_card_leaves_no_row_on_disk_and_opens_as_drawn_after_its_session` draws a card from planted replay rows, finds none of them in any file under the cache, swaps the card to light from memory, then runs `tiles open` as a second process with dark saved: it prints `Card <id> in light: <path>.png` and the line `Card <id> was drawn from Omni rows, which Tiles keeps only in the memory of the session that fetched them, so only that session can draw it again in another theme; ask for the chart again to see it in dark.`, and draws no new card. `cargo test -p tiles-mcp an_omni_card_switches_theme` covers the swap through `tiles_theme` and the line `tiles theme` prints from another process. A live Omni instance is checked by hand ([laptop-check.md](../../../../docs/laptop-check.md#omni)).

## Gotchas

- Cards drawn before this change have no saved call (`<id>.call.json`); they keep their theme, and `tiles open` in another theme says it cannot find their call.
- A call file written before ADR-027 still holds its Omni rows until the card is pruned; Tiles treats it as an Omni card and never reads the rows back.
- Claude Code's own `/theme` and Codex's own `/theme` change those harnesses' colours, not Tiles'; that is why Tiles' command has another name there.
- `tiles run` no longer reads the terminal's background to pick light or dark; a light terminal gets Obsidian until the user picks light.
