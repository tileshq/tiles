# ADR-052: The model writes the placeholder block inside a ```` ```tiles ```` fence again, because OpenCode draws a bare `[tile:]` line as a link

- Status: Accepted; supersedes [ADR-043](ADR-043-block-as-plain-lines-and-fence-blanking.md)'s plain lines, and keeps its fence blanking; that blanking is amended by [ADR-057](ADR-057-precut-fence-resize-and-waiting-rows.md), which also blanks a fence under any row of a block
- Date: 2026-10-21

## Context

[ADR-043](ADR-043-block-as-plain-lines-and-fence-blanking.md) had the model write the placeholder block as plain lines, never inside a code fence, because Pi printed a ```` ```tiles ```` line above the card and a fence below it. It also had the wrapper blank a fence line right above or below a block. Its consequences noted that no one had watched a plain-lines block in Claude Code, Codex or OpenCode with the wrapper drawing.

The owner's v0.1.10 laptop test in Warp on 2026-10-21 watched it in OpenCode (launcher 2.1.2): neither card stayed, and the reply showed lines such as `tile:b603fa 04/38` where the dashboard should have been. A diagnosis found the following.

- **OpenCode rewrites the lines.** OpenCode streams each row as text, `[tile:c40d83 01/23] Signups…`. Once its markdown parser has the paragraph, it writes the same rows again as a link: teal, underlined, both brackets gone, two spaces where they were. OpenCode 1.19.3 on the development Mac wrote `ESC[38;2;86;182;194m ESC[4m tile:d1a742 20/23` for every row, 416 such tokens in one session.
- **The wrapper loses the block.** The wrapper finds a block only by its bracketed tokens (`tiles_term::block::find`). A rewritten row holds none, so the card's copy drops out and its pictures are deleted.
  - On the laptop, OpenCode rewrote the rows on nearly every paint while the reply streamed, so the line card showed in 10 of 51 frames and the dashboard in 4 of 36.
  - On 1.19.3 the line card stayed 4.8 s, then went when the reply ended.
- **A fence fixes it.** OpenCode shows a fenced block's rows as code, brackets kept, and never writes them again. With v0.1.9, which asked for the fence, the same OpenCode in the same Warp window kept its card, and wrote no link-styled token.

Pi's fence lines, the reason for ADR-043, no longer need the plain lines. Since ADR-043 the wrapper blanks a fence line right beside a block in every harness it draws for.

## Decision

- **The block goes in a fence.** Under `TILES_DRAW=block`, the session instruction (`instructions::session`) and the tool result (`card::text_result`) ask the model to put the fenced block in its reply exactly as written, fence lines included. The tool result shows the block between a ```` ```tiles ```` line and a ```` ``` ```` line. Every harness gets the same words, as before.
- **Fence blanking stays.** The wrapper still blanks a code fence line on the row right above a block's first row or right below its last, and nothing else, as ADR-043 set out. A harness that prints the fence lines (Pi) shows a clean card; one that hides them (OpenCode) has nothing to blank.

## Consequences

- OpenCode keeps its cards: the rows stay bracketed, so the wrapper finds the block on every paint and after the reply ends.
- A model that writes the block as plain lines anyway still loses its card in OpenCode, the same way; the tool result shows the fence to copy.
- Pi indents a fenced block's lines two columns. Tokens are found at any column, and a fence at or left of the tokens' column is blanked, so Pi draws as it did under ADR-043 once the block is whole. 2026-10-22: not while it streams. Pi draws a closing fence under the last row typed so far, and two of its backticks showed left of the card, a row lower with each row typed, in v0.1.12 on the laptop; [ADR-057](ADR-057-precut-fence-resize-and-waiting-rows.md) blanks a fence under any row of a block.
- On the development Mac in Warp, OpenCode 1.19.3 with this change drew the signups card and the four-card dashboard, and both stayed after their replies ended, with no link-styled token written. Claude Code 2.1.306 and OpenCode 1.19.3 in print mode put all 14 lines of a card inside the fence. The fenced block has not yet been seen on the laptop with Claude Code, Pi and the OpenCode launcher; the next laptop check looks for a card that stays after each reply ends and for no `tile:` link text.
- Tests: `the_block_instruction_asks_for_the_block` in `crates/tiles-mcp/src/instructions.rs`; `the_block_text_carries_every_row_token` in `crates/tiles-mcp/src/card/tests.rs`; `claude_gets_its_own_launch_options` in `crates/tiles-cli/src/harness.rs`; and, replaying the rows OpenCode wrote in Warp, `precut_an_opencode_fenced_block_keeps_its_card_when_the_reply_ends` and `precut_an_opencode_block_drawn_again_as_links_is_no_block` in `crates/tiles-cli/src/wrap/paint/tests.rs`.
