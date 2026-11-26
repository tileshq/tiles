# The pane prototype

`tiles pane <fixture> [--trace <file>]` takes the whole terminal and shows a fixture dashboard as separate kitty pictures: hovering a mark draws its outline and tooltip over the card, clicking a mark filters the other cards to its value, a resize lays everything out again, and `q` gives the screen back. It exists only in builds with debug assertions (a debug build, or `cargo build --profile pane -p tiles-cli`) and decides whether the interactive pane goes ahead (ADR-063). `--trace` writes the timings `scripts/pane-trace.py` turns into the go or no-go table.

## Sub-features

- `pane-first-frame` draws the dashboard's heading and eight cards, laid out as the chart tool lays them out at the terminal's width, with a status line on the last row.
- `pane-hover` draws one small picture over the card under the pointer, the mark outlined and its tooltip beside it, and sends no card again; moving off every mark removes it.
- `pane-click` filters the other cards that hold the clicked mark's region (or series) to its value and sends only those cards; the status line names the selection; clicking it again, or empty space, clears it.
- `pane-resize` lays out and draws everything again when the window changes size.
- `pane-stress` draws `fixtures/pane/table-stress.json`, a grid of 40 rows by 8 columns of text, one row further down per wheel step or arrow.
- `pane-exit` gives the screen back on `q`, Control-C, Control-D, SIGINT, SIGTERM, SIGHUP, SIGQUIT and a panic: no mode left on, no picture left, the shell's screen as it was.
- `pane-hidden` is absent from `tiles --help` and from a release build (`tiles pane` there is an unknown command).
- `pane-trace` writes the trace, and `scripts/pane-trace.py` prints the table.

## How to get to it (user POV)

- A developer runs `./target/pane/tiles pane fixtures/pane/dashboard.json` in Ghostty or kitty, full window. No harness, no multiplexer.

## Driving it with drive.sh

Preconditions:

- `drive.sh` cannot drive the pane: its tmux session shows no kitty pictures and sends no pixel mouse reports. Drive it by hand in Ghostty or kitty, or through the pseudo-terminal tests.
- Build the measured binary: `cargo build --profile pane -p tiles-cli --locked`. A debug build runs the same code too slowly to measure.

- **Pseudo-terminal.** `cargo test -p tiles-cli --test pane` drives `pane-first-frame`, `pane-hover`, `pane-click`, `pane-resize`, `pane-stress`, `pane-exit` (SIGINT and a panic) and a run with no terminal, answering the probe as a terminal with pixel mouse reports does. Nothing draws the pictures there, so it proves bytes and timings, never the look.
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
- **Hidden.** `./target/debug/tiles --help` names no `pane`; `cargo build --release -p tiles-cli && ./target/release/tiles pane x` exits 2 with `unknown command "pane"`.
- **Fuzz.** `cargo +nightly-2026-10-01 fuzz run pane_input <copy of fuzz/corpus/pane_input> -- -max_total_time=60` ends with `Done` and no crash ([fuzz/README.md](../../../../fuzz/README.md)).

## Gotchas

- A terminal that does not answer the 1016 probe gets cell mouse reports; hover then lands on the middle of each cell.
- The pictures are sent at their own pixel size; a terminal that reports no cell size in pixels (`CSI 16 t`, or the window size) falls back to 16 by 32, and cards then sit at the wrong size.
- tmux shows nothing: the pane writes kitty pictures straight to the terminal, with no passthrough.
- `--trace` samples memory and CPU once a second with `/bin/ps` on macOS, which costs a little CPU of its own; the idle row includes it.
