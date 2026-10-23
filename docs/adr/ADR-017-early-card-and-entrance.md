# ADR-017: In kitty the wrapper shows the whole card on the block's first row and plays its entrance as one kitty animation

- Status: Accepted; its entrance drawn at the card's resolution, sent with the card as one image, with every card of a row at once, is superseded by [ADR-020](ADR-020-dashboards.md): the crisp card goes first and the entrance follows as a half-resolution image of its own; its session token, set only where the wrapper plays the entrance, is set wherever the wrapper runs as [ADR-024](ADR-024-security-baseline-in-ci.md) records; its run repeated and stopped after half again, and `TILES_FRAMES` in kitty only, are superseded by [ADR-030](ADR-030-entrance-by-the-clock-on-every-route.md): frames 50 ms apart, the frame shown picked by the clock, on the pre-cut route too; its consequence that Warp keeps ADR-011's rule is superseded by [ADR-044](ADR-044-precut-card-whole-from-its-first-row.md): the pre-cut route shows the whole card from the block's first row too
- Date: 2026-10-18
- Supersedes: [ADR-011](ADR-011-surfaces-v0-1.md)'s "a card shows only once its whole block is on screen", in kitty

## Context

OpenCode types a placeholder block a row at a time, about 1.3 s for 23 rows. The wrapper drew a card only once every row of its block was on screen and sent only the settled PNG, so the owner watched an empty code block grow downward and then the card appear in one frame, with no entrance (the KPI demo's report, problem 1). The engine has drawn the entrance since ADR-010, but no surface played it. The prototype (`open/display/tiles-run.mjs`) had fixed both halves in kitty: it placed the whole card as soon as the block's first row showed, over an opaque backing in the conversation's colour, and let kitty play the entrance itself.

## Decision

- `tiles run` sets `TILES_FRAMES` and a session token, `TILES_SESSION`, for the MCP server where the wrapper draws in kitty, on the placeholder route, with no multiplexer: 15 frames for OpenCode and Codex, 22 for Claude Code (forced onto the wrapper), the prototype's counts for how long each harness takes to type its reply. The server then draws the picture's entrance, every card of a row at once (`Engine::board_entrance`, PNG at zlib level 1), and writes `<id>-anim-NN.png` beside the card before its geometry file, which now carries `frames`, `frame_ms` and `session`. Elsewhere no frame is drawn, so no other route waits for them.
- The wrapper watches the card cache and sends a new card to kitty before its block is typed, as one image: the settled card is frame 1 (what any terminal that never runs the animation shows), the entrance frames follow 10 ms short of `frame_ms` apart, then the settled card again.
- On the first sight of the block's first row, the wrapper places the whole card over the rows the block will take (a kitty placement above the text), on a flat backing the card's size in the colour of the cell beside the block (the terminal's reported default when that cell has none). It stops at the harness's footer: the first row, from the third under the typed row, with a box border or a background of its own. The typed rows' tokens are blanked under it as before.
- The card's entrance starts at the same moment: cued on its first entrance frame and run once, or, while the harness holds a synchronized update, run when the update ends (one second at most). The wrapper repeats the run every 50 ms while the entrance plays, since kitty moves on only when it redraws, and stops it on the last frame after the frames' time and half again.
- Once the block is whole and the entrance has played, the early placement and its backing are deleted, leaving the card in its placeholder cells, and the settled card alone is sent again under the same image id with its virtual placement, so kitty frees the entrance frames and holds one picture per card. A copy the harness folds (`… +20 lines` under it) is never placed early; a block that leaves the screen takes its early placement with it.

## Consequences

- A `tiles_chart` call in kitty waits for the frames as well as the card. The cost is measured in the verify-tiles `terminal-routes` feature.
- Ghostty, iTerm2, Warp, herdr and sixel terminals keep ADR-011's rule: nothing there was driven with early placement or kitty animation.
- Every wrapper polls the shared card cache every 50 ms (one `stat` when nothing changed), and sends ahead only a card made after it started whose geometry carries its own session's token; another session's cards never reach its window.
