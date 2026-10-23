# Checking Tiles on a laptop

Some routes cannot be driven on the development Mac: Warp, a launcher build of OpenCode and a live Omni instance. Apple Terminal can, but the companion window is checked there by hand too, since it is what a user of plain Terminal sees. The `verify-tiles` skill proves everything else in a tmux pane; these checks run by hand on a laptop that has them, after each release that touches those routes.

## Installing over an earlier copy

On a laptop that still has `~/.local/bin/tiles` from an earlier `install.sh`, ahead of `/usr/local/bin` on `PATH`, open the package. The setup window lists that copy with its version, says it comes first on `PATH`, and asks `Move /Users/<you>/.local/bin/tiles aside, to /Users/<you>/.local/bin/tiles-<version>-old? [y/N]` after the harness question. Answer `y`, then in a new Warp tab `tiles --version` names the package's version and `tiles doctor` prints no `comes first on PATH` line ([ADR-035](adr/ADR-035-macos-package-and-tiles-setup.md)).

## Warp

Run these in a Warp tab, one harness at a time ([ADR-025](adr/ADR-025-warp-cell-size-rows-and-laptop-check.md)). `tiles check` prints one line per check, `pass` or `fail` first, and exits non-zero when any fails. Give it the tag you installed:

```sh
tiles check --tag v0.1.1
```

It checks the version against the tag; the terminal and route; the graphics answer; the cell size `tiles run` would pass, with the `CSI 16 t` reply and the window size beside it; `TILES_ROWS` and the tallest card it allows; the entrance the route plays (`motion: entrance on the kitty-precut route, 10 frames at 50 ms`, or `reduce` or `off`); who draws for each harness on `PATH` (Pi in Warp reads "drawn by the wrapper"); for Pi, Claude Code and Codex, the version on `PATH` against the ones Tiles was proved on, failing on a major and minor version that matches none of them ([ADR-032](adr/ADR-032-update-command-and-harness-versions.md)); the OpenCode version and which server a `tiles run opencode` session's model calls; the theme in use; and, for the newest five cards, their theme, claim tone and rows. A red highlight is a bad claim tone ([card rule K2](card-rules.md)), not another theme. Piping the report into a file to send back (`tiles check --tag v0.1.1 | tee check.txt`) reports the same facts, since `tiles check` and `tiles doctor` ask `/dev/tty` when one of stdin and stdout is a terminal and the other is not ([ADR-054](adr/ADR-054-dev-tty-only-beside-a-terminal.md)).

Then for each of `claude`, `pi` and `opencode`, start the harness with a trace:

```sh
TILES_TRACE="$HOME/Downloads/tiles-trace.txt" tiles run claude
```

Record the Warp window (Cmd-Shift-5) for both prompts. Ask "Chart our weekly signups: W1 120, W2 180, W3 150, W4 240, W5 310. W3 dipped because of the outage." The card appears in the reply at about 58 columns, headline on screen, with no panel, `PNG` line, token text or line of backticks (```` ```tiles ````, ```` ``` ````) around it, and the reply keeps the block's rows apart rather than joining them into one line ([ADR-043](adr/ADR-043-block-as-plain-lines-and-fence-blanking.md)). The model puts the block inside a ```` ```tiles ```` fence, and in OpenCode the card stays after the reply ends, with no `tile:` line shown as a link in its place ([ADR-052](adr/ADR-052-block-inside-a-tiles-fence-again.md)). Then ask "Show me a mini dashboard of our business last 12 months: revenue, active users and churn as scorecards, and revenue by month as a line chart, using made-up example data."

What the recording shows ([ADR-044](adr/ADR-044-precut-card-whole-from-its-first-row.md)):

- The whole card appears at once and fades and floats in over about half a second, where its block starts. No row of it appears a band at a time while the reply is typed under it. A card typed near the bottom of a full screen shows nothing until the reply has scrolled up far enough for all of it and the card has stopped moving, or the block is done, then comes in whole and plays its entrance in one place; the dashboard plays all of it, never jumping to the crisp card halfway ([ADR-053](adr/ADR-053-precut-entrance-waits-for-a-still-card.md)).
- Press Esc while a card's rows are being typed: within 1.5 s the card is cut to the rows typed and the Interrupted line and input box show under it.
- Under the card, while its block is typed, the harness's own lines (Pi's editor, OpenCode's "Build" line) are hidden, and they come back below the card once the block is done.
- No frame shows the card's place empty, and no frame shows another card's picture there, in either theme. Step the recording (`ffmpeg -ss <t> -frames:v 1`) across the card's arrival and across a scroll.
- With a card settled, make the window three rows shorter and back: the card never blinks out, and it stays where its block stands once the harness draws again ([ADR-057](adr/ADR-057-precut-fence-resize-and-waiting-rows.md)).
- Run Claude Code, then Pi, then OpenCode in the same tab: when the later sessions' cards arrive, scroll up and check the earlier sessions' cards in the tab are unchanged.
- In the light theme (`TILES_THEME=light`), no line crosses the card.

Quit the harness and run the check again with the same trace:

```sh
TILES_TRACE="$HOME/Downloads/tiles-trace.txt" tiles check --tag v0.1.1
```

Its `trace:` line says whether the wrapper saw every card of that session whole; a card it never saw whole is one that stayed blank, and a card whose last placement, loss or scroll was `copy lost at screen row N` is one whose tokens left the screen while its block was still there ([ADR-053](adr/ADR-053-precut-entrance-waits-for-a-still-card.md)). The trace itself says when a card waited for room and how many rows it needed, when it was held until it stopped moving, and why an entrance was dropped. A session that made no card but saw others whole was answered by another session's tiles server, and the line names those cards. If a card looks stretched or too tall, the `cell size` line shows both candidates, and `TILES_CELL_PX=16x32 tiles run claude` tries a measured size for one session.

## Apple Terminal

Terminal.app shows no pictures, so cards come in a companion window beside it ([ADR-046](adr/ADR-046-companion-window-on-the-text-route.md)). In a Terminal.app window, `tiles setup --dry-run` and `tiles doctor` each print a `pictures:` line saying cards will not draw inline in this terminal, naming Warp, Ghostty, kitty, iTerm2 and WezTerm, and saying `tiles run` shows each session's cards in one window beside the terminal.

Then for each of `claude`, `pi` and `opencode`, with Terminal.app taking the left two thirds of the screen:

```sh
TILES_TRACE="$HOME/Downloads/tiles-trace.txt" tiles run claude
```

Ask for the signups chart above. When the card lands, a window titled with the card's title opens right of the Terminal window, at the card's size, dark for Obsidian. Terminal stays the active app: its title bar stays lit and the next keystroke lands in the harness. The reply says in a line that the chart is in the window beside the terminal, may give the PNG path for reference, and does not tell you to run `tiles open <id>`. Ask for the mini dashboard: the dashboard replaces the chart in the same window, which keeps its top-left corner unless the wider card would run past the screen's edge, and then shifts back so the whole window stays on the screen. Close the window with its close button, ask for one more chart: it opens again. Quit the harness: the window closes. The trace has `window: opened with <id>`, `window: swapped in <id>`, `window: closed by the user` and `window: closed as tiles run ended` lines. With `TILES_WINDOW=off tiles run claude` no window opens and the reply gives the PNG path and `tiles open <id>`.

## OpenCode's launcher

Some OpenCode builds start a launcher whose TUI attaches to a background service ([ADR-029](adr/ADR-029-opencode-launcher-standalone.md)). Where `opencode --help` lists `--standalone`, run the signups chart and the mini dashboard above through `tiles run opencode`: both cards draw, fitted, and `tiles check --tag <tag>` passes its `opencode server:` line. A `tiles run opencode --server <url>` stops with one line, since that server never loads the session's tiles server.

## Omni

The development Mac never sees a real Omni instance or key ([ADR-008](adr/ADR-008-data-sources.md)). On a laptop with Omni set up ([omni.md](omni.md)), draw an Omni chart in a `tiles run` kitty session, then check that its call file holds no row ([ADR-027](adr/ADR-027-omni-rows-stay-in-memory.md)): `grep -c '"rows"' ~/Library/Caches/tiles/cards/<id>.call.json` prints 0.

