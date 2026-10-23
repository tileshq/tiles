# Where the charts appear

`tiles run <harness>` starts Pi, OpenCode, Claude Code or Codex with Tiles loaded for that session only, and draws each chart card inline in the terminal. Nothing in the harness's own config changes. The reasons are in [ADR-011](adr/ADR-011-surfaces-v0-1.md); the install is in [install.md](install.md).

```sh
tiles run pi
tiles run opencode
tiles run claude
tiles run codex -m gpt-5           # everything after the harness name goes to the harness
```

Then ask for a chart in words ("chart our weekly signups: W1 120, W2 180, W3 150; W3 dipped because of the outage"). The model calls `tiles_chart` with a short chart intent, Tiles draws the card, and the card appears where the model's reply puts it.

## With Omni data

With Omni configured ([omni.md](omni.md)), the model also gets `tiles_omni_topics` and `tiles_omni_fields`, and can pass `data.omni` (a topic and its fields) instead of rows. Tiles fetches the rows itself; the model gets the card's title, form and id but no picture and none of the values ([omni.md](omni.md#what-reaches-the-model)). With Omni configured, the harness asks before each Tiles call unless you start it with `tiles run --allow-omni`. Ask "chart orders by week from the orders topic".

## Who draws, per terminal

`tiles doctor` prints the terminal Tiles detected, the route it picked and why, each harness on `PATH`, the card folder and the Omni setup (the API key only as "set").

| Terminal | Route | Pi | Claude Code | Codex, OpenCode |
|---|---|---|---|---|
| kitty, Ghostty | placeholders | Pi's own image | the Claude Code mod | the wrapper |
| iTerm2 | placeholders | Pi's own image | the wrapper | the wrapper |
| Warp, WezTerm, VS Code, anything inside herdr | pre-cut placements | the wrapper | the wrapper | the wrapper |
| foot, Windows Terminal | sixel | the wrapper | the wrapper | the wrapper |
| Terminal.app, Alacritty, GNOME Terminal, tmux | text | summary and PNG path | summary and PNG path | summary and PNG path |

On the text route the terminal draws no card, so on macOS `tiles run` shows each card of the session in one companion window beside the terminal ([ADR-046](adr/ADR-046-companion-window-on-the-text-route.md)). The window opens with the session's first card and takes each later one in place, at the card's drawn size and in its theme. It never takes focus: the terminal stays in front and keeps your typing. It goes right of the terminal's window when the screen has room, else left of it, else at the screen's right edge. The chart result tells the model the card is already in that window, so the reply says so and asks you to open nothing; the PNG path stays for reference. Closing it leaves the harness running, and the next card opens it again; it closes when the harness exits. It shows only the cards its own session drew. `TILES_WINDOW=off` (or `0`, `false`, `no`) keeps it closed, and each reply then gives the PNG path and `tiles open <id>`. `tiles setup` and `tiles doctor` say on a `pictures:` line that cards will not draw inline in such a terminal and name the ones that do: Warp, Ghostty, kitty, iTerm2 and WezTerm. Off macOS the text route gives the summary and the PNG path only.

Detection is explicit and you can override it: `tiles run --terminal warp claude`, `tiles run --route kitty-precut codex`, or `TILES_TERMINAL` and `TILES_ROUTE` in the environment. `--wrapper` makes the wrapper draw even where the harness would (useful if Pi's own image misbehaves). `tiles run --dry-run <harness>` prints the exact command, arguments and environment, and each file the launch would write with its contents (Claude Code's settings file), without starting anything. `TILES_CELL_PX=WxH` sets the cell size in device pixels. Without it Tiles takes the terminal's `CSI 16 t` reply, else the window size over its rows and columns, read as points in Warp, which reports points at every font, and in any other terminal when that gives a cell under 10 CSS pixels tall; `tiles doctor` prints the size and where it came from. The rule assumes a 2x display. On the kitty-precut route a card is drawn on fewer cells when it would take more than the terminal's rows less 10, so its headline stays on screen; on the sixel route a tall card is still cut at the screen's edge. `TILES_THEME=obsidian|light|dark` sets the card theme for one session, over the theme saved with `tiles theme`.

Where the wrapper draws pictures in kitty or on the pre-cut route, a card enters: it fades in and floats up 10 px while its marks grow, then its values fade in, 450 ms in 10 frames 50 ms apart; a dashboard's cards start 50 ms apart. On the pre-cut route the whole card shows from its block's first row, over the rows the block will take, while the harness types the rest under it; the frames are placed one per tick, each over the one before, and the crisp card ends it ([ADR-044](adr/ADR-044-precut-card-whole-from-its-first-row.md)). A card the harness scrolls or moves keeps its frame, or goes on to the next one when its frame is too large to send again, and never starts its entrance again; a card that waited for room starts its entrance once it stops moving, so the entrance plays in one place ([ADR-053](adr/ADR-053-precut-entrance-waits-for-a-still-card.md)). When macOS's Reduce Motion is on the card only fades in, over 150 ms; `TILES_MOTION=on|reduce|off` overrides that for one session, and `off` shows the settled card at once ([ADR-030](adr/ADR-030-entrance-by-the-clock-on-every-route.md)). If a terminal shows no entrance on the pre-cut route, `TILES_PRECUT_SEND=tick` sends each frame as it is placed.

Cards are drawn in Obsidian unless the user picks another theme. Inside a harness the `tiles_theme` tool lists and sets it: type `/mcp__tiles__theme [name]` (or `/tiles:theme` where the Tiles plugin loads) in Claude Code, `/theme [name]` in OpenCode and Pi, and `$tiles-theme` in Codex after `tiles install codex`; Claude Code and Codex keep their own `/theme` for their colours. In a `tiles run` kitty session the cards already drawn switch to the new theme in place, a card drawn from Omni rows only through the MCP server that fetched them, since those rows stay in its memory and never reach the disk ([ADR-027](adr/ADR-027-omni-rows-stay-in-memory.md)); elsewhere only new cards change, and `tiles open <id>` shows an earlier card in the new theme and prints its PNG path ([ADR-021](adr/ADR-021-obsidian-default-and-theme-switch.md)). On macOS `tiles open <id>` also shows the card, a dashboard included, in a window of its own at the size it was drawn; Cmd-W closes it, and every chart result names the command except where `tiles run`'s companion window already shows the card ([ADR-023](adr/ADR-023-open-in-a-window.md)); `TILES_WINDOW=off` keeps that window closed too.

## OpenCode with a background service

Some OpenCode installs are a launcher whose TUI attaches to a background service by default: its `opencode --help` lists `--standalone`, and `opencode --version` prints `opencode v2.0.32` or similar. The service started earlier with its own configuration, so it never loads the tiles server a `tiles run` session brings, and the model draws no card or one from an older session. `tiles run opencode` reads `opencode --help` and, for such a launcher, starts OpenCode with `--standalone`, a private server that loads the session's tiles server ([ADR-029](adr/ADR-029-opencode-launcher-standalone.md)). `--server <url>` is refused there for the same reason. sst/opencode's own CLI starts a server per TUI and gets no extra flag. `tiles check` and `tiles doctor` print the OpenCode version, which of the two CLIs it is, and the version Tiles was proved on. When `~/.config/opencode/config.json` holds a tiles server too, as `tiles install opencode` writes it, `tiles check` adds an `opencode install:` line: it passes on sst/opencode, which merges the session's server over the installed one, and fails on the launcher, where Tiles has not proved which of the two the private server keeps; move the file aside or read a traced session's `trace:` line.

## Permanent install

`tiles install <harness>` adds Tiles for sessions started without `tiles run`; `tiles setup` runs it for every harness on `PATH` after one question ([install.md](install.md), [ADR-035](adr/ADR-035-macos-package-and-tiles-setup.md)). Pi gets the extension in `~/.pi/agent/extensions/tiles/`; OpenCode gets `~/.config/opencode/config.json` (refused if you already have one of your own); Claude Code and Codex get the MCP server through their own `claude mcp add` and `codex mcp add`. `--dry-run` shows the steps first. Outside `tiles run` no wrapper draws, so those sessions get the summary and the PNG's path, except Pi, which draws the image itself.

## Checks in Warp

Warp cannot be driven on the development Mac, so its checks run by hand in a Warp tab: [laptop-check.md](laptop-check.md).

## Where the files are

Cards are written owner-only to `~/Library/Caches/tiles/cards/<id>.png` (the newest 300 kept); `TILES_CACHE_DIR` moves them. Each session's add-ons go under `~/Library/Caches/tiles/run/<pid>/` and are removed when the session ends. `tiles cache clear` removes every card with its call and entrance frames and prints how many cards went; it leaves a running session's add-ons alone.

The model's own file and shell tools are kept out of the cache where the harness lets a launch say so ([ADR-028](adr/ADR-028-keep-the-model-out-of-the-cache.md)). `tiles run claude` passes `--settings` with a file in the session's add-ons that denies `Read` and `Bash` on the cache; a `--settings <file-or-json>` of your own is merged into that file, since Claude Code takes one, and the launch names only the merged file. `tiles run opencode` sets OpenCode's `external_directory` permission to `deny` for the cache in `OPENCODE_CONFIG_CONTENT`; OpenCode checks that permission only for paths outside the directory it started in, so a session started in your home directory still reads the cache. Codex and Pi get no such rule. Pi has no tool approvals, so its Tiles extension asks with Pi's own dialog (`ctx.ui.select`) before a chart that fetches rows from Omni, unless `tiles run pi` was started with `--allow-omni`; [omni.md](omni.md#approving-omni-calls) has the details.
