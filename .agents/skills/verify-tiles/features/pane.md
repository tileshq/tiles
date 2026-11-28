# The pane

`tiles pane <intent.json | card id> [--trace <file>]` takes the whole terminal and shows a chart call's cards, or a drawn card's, as separate kitty pictures, with rows the call holds or rows fetched from Omni or a warehouse and kept in memory. Hovering a mark draws its outline and tooltip over the card; clicking a mark focuses its value on every card that holds it; clicking a legend entry hides that series; `s` sorts the card under the pointer; `d` drills an Omni date card one timeframe finer and `u` goes back; `r` clears everything picked; a resize lays everything out again, and `q` gives the screen back. It is in every build (ADR-063, ADR-065, ADR-070). `--trace` writes the timings `scripts/pane-trace.py` turns into the gate table.

## Sub-features

- `pane-first-frame` draws the page colour under everything, the dashboard's heading and eight cards, every line (the three scorecards too) spanning the window less two columns each side, and a coloured status line on the last row with the keys at the right.
- `pane-hover` draws two small pictures over the card under the pointer, the mark washed lighter in a one-pixel accent ring and, as a picture of its own, a raised tooltip clear of the card's title and value labels (its name, its values and what a click does), and sends no card again; moving off every mark removes it.
- `pane-click` focuses the clicked mark's region (or series) in the accent on every card that highlights by it, every card keeping all its rows, and sends only the cards that changed and the heading, which shows the value's chip; the status line names the selection and counts the cards; the tooltip stays on the mark under the pointer; clicking it again, or empty space, clears it.
- `pane-resize` lays out and draws everything again when the window changes size.
- `pane-stress` draws `fixtures/pane/table-stress.json`, a grid of 40 rows by 8 columns of text, one row further down per wheel step or arrow.
- `pane-exit` gives the screen back on `q`, Control-C, Control-D, SIGINT, SIGTERM, SIGHUP, SIGQUIT and a panic: no mode left on, no picture left, the shell's screen as it was.
- `pane-legend` hides the series of a clicked legend entry on its card alone and sends only that card; the status line counts it; `r` brings it back. A card's last series stays.
- `pane-sort` steps the card under the pointer through largest first, smallest first and its own order with `s`, skipping a step that changes nothing; a bar chart, lollipop or time chart says it keeps its own order.
- `pane-drill` asks Omni, on `d` over a period of a card drawn from an Omni date field, for the next finer timeframe and draws only that period's rows; `u` goes back up; a card with rows of its own says what a drill needs.
- `pane-card` opens a card the chart tool drew, by its six-hex id; a missing id is refused in one line.
- `pane-release` is in `tiles --help` and in a release build.
- `pane-trace` writes the trace, and `scripts/pane-trace.py` prints the table.

## How to get to it (user POV)

- A user runs `tiles pane call.json`, or `tiles pane <id>` with an id a chart result named, in Ghostty or kitty, full window. No harness, no multiplexer.
- A developer measuring runs `./target/pane/tiles pane fixtures/pane/dashboard.json`.

## Driving it with drive.sh

Preconditions:

- `drive.sh` cannot drive the pane: its tmux session shows no kitty pictures and sends no pixel mouse reports. Drive it by hand in Ghostty or kitty, or through the pseudo-terminal tests.
- Build the measured binary: `cargo build --profile pane -p tiles-cli --locked`. A debug build runs the same code too slowly to measure.

- **Pseudo-terminal.** `cargo test -p tiles-cli --test pane` drives `pane-first-frame`, `pane-hover`, `pane-click`, `pane-legend`, `pane-sort`, the refused drill of `pane-drill`, `pane-card`, `pane-resize`, `pane-stress`, `pane-exit` (SIGINT and a panic) and a run with no terminal, answering the probe as a terminal with pixel mouse reports does. Nothing draws the pictures there, so it proves bytes and timings, never the look. `cargo test -p tiles-cli --lib pane` drives a drill through the Omni replay server.
- **The look by hand** (`pane-first-frame`, `pane-hover`, `pane-click`). In Ghostty or kitty, run `tiles pane fixtures/pane/dashboard.json`, full window and then half a screen wide. The page is one colour behind the cards, the three scorecards end on the same left and right edges as the charts under them, and the keys sit at the right of the status line. Point at France's bar: a thin accent ring, and below it a tooltip reading France, its revenue and "click to focus France". Click it: France turns to the accent on the bars, the columns, the lines and the lollipop, every card keeps its five regions, the heading shows `region France ×` and `r resets`, and the tooltip says "click again to clear".
- **Interaction by hand** (`pane-legend`, `pane-sort`, `pane-drill`). In Ghostty or kitty, run `tiles pane fixtures/pane/dashboard.json`. Click "Card" in the stacked bars' legend: Card's segments go and the status line says `1 hidden`. Point at the column chart and press `s` twice, then a third time: smallest first, then the order it came in. Press `r`. For a drill, with Omni configured, draw a column chart of a topic's `created_at[month]` and a count, then `tiles pane <its id>`, point at a month and press `d`: its weeks, subtitled `<month> by week`; `u` goes back.
- **Go or no-go in Ghostty** (`pane-trace`). In Ghostty, in a window half the width of a Retina screen, run:

  ```sh
  ./target/pane/tiles pane fixtures/pane/dashboard.json --trace /tmp/pane-dashboard.trace
  ```

  Move the pointer across every card for about 30 seconds, resting on bars, points and dots. Click Germany's bar, then a France point on the line chart, then empty space. Drag the window narrower and back. Leave the pointer still for 20 seconds, then press `q`. Then run:

  ```sh
  ./target/pane/tiles pane fixtures/pane/table-stress.json --trace /tmp/pane-stress.trace
  ```

  Turn the wheel one step at a time about 30 times, leave it still for 10 seconds, press `q`. Then:

  ```sh
  python3 scripts/pane-trace.py /tmp/pane-dashboard.trace /tmp/pane-stress.trace --machine work --label Ghostty
  ```

  Use `--machine intel` on the 2017 Intel Mac. Every row says go, miss or recorded. While moving the pointer, record the screen at 60 fps (QuickTime, a screen recording of the window) and step through it frame by frame: the tooltip follows the pointer within 2 frames and no card flickers. The shell's screen comes back after `q` with the prompt where it was.
- **Warp.** In Warp, open a split by hand (Cmd-D), run the dashboard command with `--trace /tmp/pane-warp.trace`, hover and click as above, and run the script with `--label Warp`. Record whether hover draws, whether a click replaces cards cleanly, and the hover row.
- **Orca's terminal.** On an Orca release whose terminal draws inline images (v1.4.197 does not; the current tags do), run `orca terminal split` and then `orca terminal create --command "$PWD/target/pane/tiles pane $PWD/fixtures/pane/dashboard.json --trace /tmp/pane-orca.trace"`, and check the same things with `--label Orca`.
- **Release** (`pane-release`). `cargo build --release -p tiles-cli && ./target/release/tiles --help` names `tiles pane`, and `./target/release/tiles pane x` exits 1 with `tiles pane: cannot open x: ...`.
- **Fuzz.** `cargo +nightly-2026-10-01 fuzz run pane_input <copy of fuzz/corpus/pane_input> -- -max_total_time=60` ends with `Done` and no crash ([fuzz/README.md](../../../../fuzz/README.md)).

## Gotchas

- A terminal that does not answer the 1016 probe gets cell mouse reports; hover then lands on the middle of each cell.
- The pictures are sent at their own pixel size; a terminal that reports no cell size in pixels (`CSI 16 t`, or the window size) falls back to 16 by 32, and cards then sit at the wrong size.
- tmux shows nothing: the pane writes kitty pictures straight to the terminal, with no passthrough.
- `--trace` samples memory and CPU once a second with `/bin/ps` on macOS, which costs a little CPU of its own; the idle row includes it.
