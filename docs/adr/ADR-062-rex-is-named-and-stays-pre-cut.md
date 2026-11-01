# ADR-062: Tiles names Rex and draws there on the pre-cut route

- Status: Accepted; adds Rex to the pre-cut row of [ADR-004](ADR-004-surfaces.md) and retires Rex as [ADR-060](ADR-060-setup-prints-a-step-list.md)'s example of a terminal Tiles cannot name
- Date: 2026-11-01

## Context

Rex, Superlogical's terminal, answers the kitty graphics query, so `tiles run` drew there on the pre-cut route with the terminal reported as `unknown`. Tiles never named it, with three results:

- `tiles doctor`, `tiles check` and any report run by an agent's tool, where nothing can be asked of the terminal, said `unknown` and the text route.
- A reply later than the 600 ms `tiles run` waits dropped the session to the text route.
- A stray `GHOSTTY_RESOURCES_DIR` called Rex Ghostty, which picks the placeholder route and, for Claude Code, the mod.

Public reports dated 2026-10-27 to 2026-11-01 give Rex's identity: `TERM_PROGRAM=rex` throughout, `TERM=xterm-256color` on Macs by default and `xterm-rex` after `rex terminfo setup`, and an XTVERSION reply starting `rex `. Reports from 2026-10-25 on have Unicode placeholders working in Rex, but none of them ran Tiles, and Rex is not on the development Mac. A tmux or herdr started inside Rex rewrites `TERM_PROGRAM` and inherits the `REX_*` variables.

## Decision

- **Environment.** `TERM_PROGRAM=rex` or `TERM=xterm-rex` names Rex. Each is an exact match, so `TERM=xterm-rex-extra` and `TERM_PROGRAM=rexy` do not. This is the first branch of `terminal_from_env`, so a stray `GHOSTTY_RESOURCES_DIR` or `KITTY_WINDOW_ID` cannot win.
- **Never `REX_*` alone.** A multiplexer inside Rex inherits them, and they say nothing about what is drawing.
- **Replies.** An XTVERSION reply starting `rex ` names Rex where the environment does not, as over SSH when `TERM` falls back.
- **Route.** Rex's native route is `KittyPrecut`, what it already got. ADR-004 moves a terminal to the placeholder route only when a test in Tiles shows placeholders working there, so the move waits on that test in Rex and a new record.
- **Multiplexers.** The rules are unchanged: tmux in Rex stays on text and herdr in Rex on pre-cut.
- **Who draws.** The wrapper, for every harness, with the entrance a frame per tick as on the rest of the pre-cut route. The Claude Code mod and Pi's own image stay where they were proved.

`crates/tiles-term/src/detect.rs` holds the rules and their tests; `rex_draws_a_placed_picture_without_placeholders` in `crates/tiles-cli/tests/pty.rs` checks the bytes under `--terminal rex`.

## Consequences

- `tiles doctor` and `tiles check` say `rex, route kitty-precut` with no replies, and `tiles setup` says `rex: nothing to set up`. A slow reply no longer drops Rex to text. The `pictures:` line on the text route lists Rex with the terminals that draw inline.
- No picture has been seen in Rex with this change: the tests check Tiles' choices and its bytes, and whether a card survives closing a Rex window and reattaching is still unknown.
- A program started from a Rex shell inherits `TERM_PROGRAM=rex` unless it sets its own. kitty does not set it, so a kitty window opened from Rex reads as Rex and takes pre-cut, which kitty also draws. `--terminal kitty` or unsetting the variable restores it.
- Another terminal started from a Rex shell that keeps `TERM=xterm-rex` also reads as Rex until `--terminal` overrides it.
- Related: [ADR-004](ADR-004-surfaces.md), [ADR-011](ADR-011-surfaces-v0-1.md), [ADR-046](ADR-046-companion-window-on-the-text-route.md), [ADR-060](ADR-060-setup-prints-a-step-list.md).
