# ADR-004: v0.1 draws in Pi, OpenCode, Claude Code and Codex, in Warp, Warp inside herdr and ordinary terminals; official hooks first, the wrapper as fallback

- Status: Accepted; its harness routes and the floating window are superseded by [ADR-011](ADR-011-surfaces-v0-1.md)
- Date: 2026-10-17

## Context

The owner runs Pi in Warp inside herdr and Claude Code in Warp, and OpenCode is common among the people Tiles is for. On 2026-10-17 at 22:20 he added: "this should also run on normal terminal not just warp, opencode, pi, codex." The prototype's `tiles run` wrapper puts a harness in a pseudo-terminal and rewrites what it draws. It works, but any harness release can break it (Claude Code shipped 261 releases in 2026, OpenCode 240, Codex 107). The harnesses now offer official routes, each with limits:

| Harness | Official route | Limit found |
|---|---|---|
| Pi | An extension's `registerToolRenderer` picks the renderer for any tool, MCP tools included. Pi draws tool-result images itself over kitty graphics and iTerm2 images (since 0.21.0) | Round 3 confirms it in Warp and in herdr |
| OpenCode | A CLI plugin's `registerCodeBlockRenderer(lang, ...)` renders a fenced block; MCP for the tool | Image support inside OpenTUI is not confirmed |
| Claude Code | MCP tool; a mod's `ui.render` hook can replace the tool's result row with an `Image` element | `Image` shows pixels only in kitty and Ghostty, alt text elsewhere, Warp included. MCP tool images are saved to a file, not drawn |
| Codex | MCP tool; hooks exist | No render hook found. Its TUI shows "Returned image" as text |

What the prototype rounds learned about terminals: Warp supports kitty graphics but not Unicode placeholders, and ignores kitty crop rectangles (round 9), so a partly visible card must be sent pre-cut. herdr 0.9.0 ignores the row diacritic on placeholder cells, and in Warp a moved image leaves a copy at each scroll step, so an image is deleted and placed again with a new id instead of moved.

## Decision

Every surface is an adapter over one engine call: intent in, card out (PNG bytes, or raw RGBA where the terminal takes it, plus a text fallback).

### Harnesses

| Harness | v0.1 route | Fallback |
|---|---|---|
| Pi | An extension that registers the renderer for the Tiles tool and shows the card through Pi's own tool-result image | The wrapper |
| OpenCode | The MCP tool, plus a CLI plugin that renders a one-line `tiles` fence the model writes | The wrapper, if OpenTUI cannot show the image |
| Claude Code | The MCP tool, with the inline terminal route through the wrapper (`tiles run claude`) wherever its own `Image` shows only alt text, Warp included | The terminal fallback below |
| Codex | The MCP tool's image content, with the inline terminal route through the wrapper (`tiles run codex`), since Codex has no render hook | The terminal fallback below |

The MCP tool always returns an image content block and a short text result (the title and the values on the card), so every host gets the picture or the numbers.

### Terminals

Tiles detects the terminal from its replies (kitty graphics query, XTVERSION, device attributes) and `TERM_PROGRAM`, and picks the best route it supports:

| Route | Terminals | How the card shows |
|---|---|---|
| Kitty graphics with Unicode placeholders | kitty, Ghostty, iTerm2 | Placeholder cells carry the image, so it scrolls and survives redraws |
| Kitty graphics, pre-cut | Warp, herdr inside Warp, VS Code's terminal with images turned on, WezTerm (partial kitty support) | Placed images without placeholders. A partly visible card is sent as a new image of its visible part; a moved card is deleted and placed again. A terminal moves to the placeholder route only when a test in Tiles shows placeholders working there |
| Sixel | Terminals with sixel and no kitty graphics (Windows Terminal, foot, mlterm, xterm built with sixel) | One sixel picture, redrawn when it scrolls back into view |
| No inline images | Terminal.app, Alacritty, stock GNOME Terminal | On macOS, the prototype's floating window placed over the reserved rows. Everywhere, a clean text summary (title and values) plus the path of the saved PNG. Later, the companion window |

Each rule is encoded once in the terminal output module with a test per terminal route, never repeated in adapter code. Everything Tiles writes to a terminal passes through one encoder and one text sanitizer (ADR-009).

The wrapper keeps the prototype's paint lessons: no painting in the middle of an escape sequence, synchronized output, restoring the cells it covers, ignoring folded blocks. It is a compatibility layer: a harness that gains an official route that draws in a terminal moves off it there.

### Later

- A Claude Code mod drawing the card through `Image` and swapping entrance frames with `$.ui.blit`, for kitty and Ghostty users and for Warp once Claude Code draws images there.
- MCP App HTML output for Claude desktop, ChatGPT, VS Code and Goose.
- The companion window beside the terminal (stage 4), which also becomes the main fallback for terminals without images.
- Grok through the wrapper.

## Consequences

- Four harnesses and the wrapper call the same engine API, so the engine round's output serves each without change.
- Claude Code and Codex depend on the wrapper in Warp in v0.1. That is the most fragile path, so it gets the most terminal tests in the verification skill (ADR-006).
- The terminal matrix is a test matrix: each route has a recorded-bytes test, and the verification skill drives one harness per route.

Sources: the startup architecture review of 2026-10-17, sections 3 and 5 (Claude Code mods reference and type declarations, Pi extensions docs and 0.21.0 release, OpenCode CLI plugin docs, Codex TUI source, Warp changelog, terminal support table, fetched 2026-10-17); the prototype's round reports (herdr, Warp and Terminal.app findings). Both are private research notes.
