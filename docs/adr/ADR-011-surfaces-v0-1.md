# ADR-011: `tiles run` is the front door: each harness gets Tiles through its own launch options, the card travels as a placeholder block, and the wrapper draws only where no hook does

- Status: Accepted; its image content in chart results, its tool pre-approval and its read-only annotation of every tool are superseded by [ADR-013](ADR-013-data-stays-local.md); its rule that a card shows only once its whole block is on screen is superseded in kitty by [ADR-017](ADR-017-early-card-and-entrance.md) and elsewhere by [ADR-025](ADR-025-warp-cell-size-rows-and-laptop-check.md), which also supersedes its Pi row; its OpenCode row is amended by [ADR-029](ADR-029-opencode-launcher-standalone.md), which adds `--standalone` for an OpenCode launcher with a background service; its rule that the block goes inside a ```` ```tiles ```` fence was superseded by [ADR-043](ADR-043-block-as-plain-lines-and-fence-blanking.md), which had the model write it as plain lines, and is restored by [ADR-052](ADR-052-block-inside-a-tiles-fence-again.md); its text route with no window is amended by [ADR-046](ADR-046-companion-window-on-the-text-route.md), which adds the companion window on macOS
- Date: 2026-10-18

## Context

ADR-004 chose official hooks first and the wrapper as fallback. On 2026-10-17 at 23:30 the owner asked whether a `tiles run` front door would beat a plugin people must install. The answer he kept: the add-ons are TypeScript, `tiles run <harness>` is the front door, it loads Tiles through the harness's own launch option where one exists, falls back to the screen wrapper where none draws, and a permanent install stays an option. Round 3 read each harness's current extension API before building on it (sources below) and found three things ADR-004 assumed differently:

| Harness | ADR-004 assumed | What the installed version does |
|---|---|---|
| Pi 1.0.0 | `registerToolRenderer` | No such call. A tool's own `renderCall` and `renderResult` render its row, and Pi draws any image content in a tool result over kitty graphics or iTerm2 images, Warp included (`terminal-image.js` detects `TERM_PROGRAM=WarpTerminal`). Pi also speaks MCP natively, with MCP tools hidden behind "codemode" unless `exposure: "direct"` |
| OpenCode 1.19.3 | `registerCodeBlockRenderer(lang, ...)` | Exists only on the unreleased `dev` branch. 1.19.3 turns MCP image content into a model attachment, draws only `⚙ tiles_tiles_chart {args}`, and its plugin hooks can neither draw a result nor add an MCP server |
| Claude Code 2.1 | MCP images saved to a file | True, and it also hands a tool's `structuredContent` to the model in place of the tool's text, which drops anything the model must copy |
| Codex 0.163.0 | No render hook | Confirmed: an MCP image shows as the text `Returned image` (`tui/src/history_cell/mcp_result.rs`) |

## Decision

### One tool, one result

The MCP server (`tiles mcp`, crate `tiles-mcp`) offers `tiles_chart`, whose input schema is generated in `tiles-intent` from the parser's own constants and enums, with a test that reads the parser's accepted keys from its refusal of an unknown key, so a key added to the parser fails CI until the schema names it. With Omni configured it also offers `tiles_omni_topics` and `tiles_omni_fields` (metadata only), and an intent may name `data.omni`; the server fetches the rows itself and returns only the values on the card (ADR-005). Every result is a text block, then the PNG as image content, and no `structuredContent`. The text names the card id, its cells and the saved PNG's path. Every tool is annotated read-only, so Codex runs it without a prompt.

### The placeholder block

Where the wrapper draws, the text asks the model to copy a block of one token per row (`[tile:<id> NN/RR]`, the title on row 1) into its reply inside a ```` ```tiles ```` fence. The harness then reserves the card's rows itself, in its own scrollback, and the wrapper finds the tokens on screen and draws over them. This is the prototype's protocol, kept because it is the only one that works in every harness without its cooperation.

### Who draws

| Harness | `tiles run` loads Tiles through | Who draws the card |
|---|---|---|
| Pi | `-e <session add-on>`: an extension that registers `tiles_chart`, runs `tiles card` and adds the instruction as a system-prompt section | Pi's own tool-result image; the wrapper only on sixel terminals or with `--wrapper` |
| Claude Code | `--mcp-config`, `--append-system-prompt`, `--allowedTools`, and in kitty and Ghostty `--plugin-dir <mod>` | the mod's `Image` element in kitty and Ghostty; the wrapper elsewhere, Warp included |
| Codex | `-c mcp_servers.tiles.*` and `-c developer_instructions` | the wrapper |
| OpenCode | `OPENCODE_CONFIG_CONTENT`: the server, its tools allowed, and the instruction file; `--standalone` for a launcher with a background service (ADR-029) | the wrapper |

On a terminal with no images every harness runs unwrapped and the server answers with the text route: the summary and the PNG's path. ADR-004's macOS floating window for those terminals is not in v0.1; it waits for the companion window (stage 4).

OpenCode's add-on is that config layer, not a plugin: `tiles install opencode` writes it as a `config.json` of Tiles' own beside the user's `opencode.json`, which OpenCode merges key by key. When a release ships `registerCodeBlockRenderer`, a plugin that draws the ```` ```tiles ```` block replaces the wrapper there.

### Terminal routes

`crates/tiles-term` holds every terminal rule once: detection (overrides `TILES_TERMINAL` and `TILES_ROUTE` or the `--terminal` and `--route` flags, then the environment, then the replies to the kitty graphics query, XTVERSION, `CSI 16 t`, `OSC 11` and device attributes), the one kitty and sixel encoder, the block and the text sanitizer. The routes are ADR-004's, with these rules from the prototype: placeholders in kitty, Ghostty and iTerm2; pre-cut placements in Warp, WezTerm, VS Code and anything inside herdr (herdr ignores the placeholder row mark; Warp ignores crop rectangles and keeps old copies, so a partly visible card is a new picture of its visible rows and a moved card goes up under a new id before the old one is deleted); sixel in foot and Windows Terminal; text in Terminal.app, Alacritty, VTE terminals and under tmux or screen.

### The wrapper

`tiles run` runs the harness in a pseudo-terminal (`pty-process`), follows its screen with `vt100`, and paints only at ground state between escape sequences and UTF-8 characters, inside synchronized output unless the harness has it on, restoring the harness's cursor and attributes after. A card shows only once its whole block is on screen; one copy per card is drawn; tokens not drawn over and half-typed tokens are blanked. The startup query reads the terminal's replies one byte at a time and stops at the device attributes reply, so keys typed after it stay unread for the next reader: the harness itself when it runs directly, or the wrapper's key reader, which starts only once the wrapper runs.

## Consequences

- New dependencies, each permissive: `pty-process` (MIT) for the pseudo-terminal and controlling tty, `vt100` (MIT) for the screen model, and `rustix`'s `termios`, `stdio` and `event` features (Apache-2.0 or MIT, already in the lock) for raw mode, window size and polling the startup replies, so no crate of ours needs `unsafe`.
- The terminal geometry reaches the MCP server as `TILES_COLS`, `TILES_CELL_PX`, `TILES_THEME` and `TILES_CACHE_DIR`, fixed at launch: a card made after a resize keeps the launch width until the next `tiles run`.
- Cards live in the per-user cache (`~/Library/Caches/tiles/cards`, owner-only, the newest 300 kept), as `<id>.png` beside `<id>.json` holding its cells.
- Warp cannot be driven on the development Mac. Its checks are in `docs/laptop-check.md` for the owner to run; until he does, the Warp and herdr-inside-Warp rows rest on the prototype's rounds and on the pre-cut route working in kitty.

Sources: Pi `@earendil-works/pi-coding-agent` 1.0.0 `docs/extensions.md`, `dist/core/extensions/types.d.ts`, `dist/modes/interactive/components/tool-execution.js`, pi-tui `dist/terminal-image.js`; OpenCode v1.19.3 `packages/core/src/v1/config/mcp.ts`, `packages/opencode/src/config/config.ts`, `packages/opencode/src/session/tools.ts`, `packages/plugin/src/tui.ts`, and `dev` `packages/plugin/src/tui/context.ts`; Claude Code 2.1.302 mod types (`claude-code.d.ts`: `ui.render`, `ToolResult`, `ImageProps`) and a `-p --output-format stream-json` session; Codex `rust-v0.163.0` `utils/cli/src/config_override.rs`, `config/src/mcp_types.rs`, `core/src/mcp_tool_call.rs`, `tui/src/history_cell/mcp_result.rs`; all read 2026-10-18. The prototype's lessons: its `tiles run` wrapper source and its ninth-round fixes report (private research notes).
