# ADR-043: The model writes the placeholder block as plain lines, and the wrapper blanks a code fence right beside a block

- Status: Accepted; supersedes [ADR-011](ADR-011-surfaces-v0-1.md)'s rule that the block goes in the reply inside a ```` ```tiles ```` fence; its plain lines are superseded by [ADR-052](ADR-052-block-inside-a-tiles-fence-again.md), which puts the block back in the fence because OpenCode draws a bare `[tile:]` line as a link, and keeps its fence blanking
- Date: 2026-10-20

## Context

[ADR-011](ADR-011-surfaces-v0-1.md) has the model copy the placeholder block into its reply inside a ```` ```tiles ```` fence, and the tool result showed the block inside one. In a laptop test on 2026-10-19, Pi with the wrapper drawing (claude-opus-5-5) did just that, and the card showed with a line reading ```` ```tiles ```` above it and a line of three backticks below it. Pi 1.0.0's markdown renderer (pi-tui `components/markdown.js`) prints a code block's fence lines and indents its lines two columns. It also keeps a paragraph's line breaks (`applyTextWithNewlines`), so a block written as plain lines still takes one row per token there.

## Decision

- **Plain lines.** Under `TILES_DRAW=block`, the session instruction (`instructions::session`) and the tool result (`card::text_result`) ask the model to put the block in its reply exactly as written, as plain lines of their own, never inside a code fence. The result shows the block after a blank line with no fence around it. Every harness gets the same words, as before.
- **Fence blanking.** The wrapper blanks a code fence line on the screen row right above a block's first row or right below its last: three or more backticks and at most one word after them, nothing else on the row left of the conversation's edge, starting at or left of the token's column. The blank works like a token's: it takes the background beside it, and the harness's text comes back when the block leaves. A fence a row further away, a row that holds more than a fence, and a fence right of the block all stay as the harness wrote them, so a model that fences anyway shows a clean card and no other text is hidden.

## Consequences

- A card in Pi shows no fence lines whether or not the model fences the block.
- In a harness that joined a paragraph's lines into one row, a block written as plain lines would take one row and draw no card. No one has yet watched a plain-lines block in Claude Code, Codex or OpenCode with the wrapper drawing, so the laptop check in `docs/laptop-check.md` now looks for the card and for fence lines in each harness.
- A half-typed closing fence (one or two backticks) can show for a moment below the block until the third is typed.
