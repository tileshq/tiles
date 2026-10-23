# ADR-013: A chart result carries no picture, an Omni card's values stay local, and the Omni key is bound to its host

- Status: Accepted; its consequence that Omni tools run in Pi without a prompt is superseded by [ADR-028](ADR-028-keep-the-model-out-of-the-cache.md), whose Pi extension asks first
- Date: 2026-10-18

## Context

The security review of `main` at `776d462` found that every `tiles_chart` result sent the whole PNG to the model provider as image content, plus up to 12 value lines, Omni cards included, while `docs/omni.md` said the rows never reach the model (finding F2). Everything in a tool result goes to the provider: the Claude Code, Codex and OpenCode hosts pass MCP content to the model, and Pi's `docs/extensions.md` calls a result's `content` model-facing. No MCP host drew the card from that image: the wrapper draws from the card cache, the Claude Code mod from the saved file, and the text route gives the path. Only Pi drew it, from image content.

`tiles run` also pre-approved `tiles_chart` and the two Omni tools in Claude Code and OpenCode, and Codex runs a read-only tool without asking, so a model steered by text it read could pull any topic the key sees with no prompt.

The Omni URL and key were each read "environment first, then Keychain", so an environment `TILES_OMNI_URL` sent the Keychain key to any HTTPS host (F3), and the documented `security add-generic-password` recipe made an item any process could read silently, since `/usr/bin/security` created it and is trusted by its access list (F4).

## Decision

- A `tiles_chart` result is text only: no image content and no `structuredContent`. This supersedes ADR-011's "a text block, then the PNG as image content".
- Pi's extension returns the card's file path in the result's `details`, which Pi keeps for rendering, and its `renderResult` draws the card with pi-tui's `Image`. Pi no longer draws from image content.
- The text lists the values on the card only when the model sent the rows itself. For a card drawn from Omni rows it holds the title the model wrote, the form, the id and the PNG path, and says the values stayed on the machine. `TILES_OMNI_VALUES_TO_MODEL=1` in the server's environment adds the values back. This is option A of the decision held for the owner; option B is that setting, and option C (the picture as well) is no longer offered.
- With Omni configured, `tiles run` pre-approves nothing: Claude Code gets no `--allowedTools`, OpenCode gets `ask` for every Tiles tool, and Codex gets `default_tools_approval_mode = "prompt"` for the server (accepted by codex-cli 0.163.0, which refuses any other value outside `auto`, `prompt`, `writes` and `approve`). `tiles run --allow-omni` restores the old pre-approval. Without Omni, `tiles_chart` stays pre-approved, since it can only draw rows the model typed. A server started without Omni refuses Omni intents for its whole session.
- The same holds outside `tiles run`. With Omni configured, the MCP server publishes `tiles_chart` and both Omni tools with `readOnlyHint: false` and `openWorldHint: true`, so a client that honours the hints asks first; without Omni, `tiles_chart` keeps `readOnlyHint: true`. This supersedes ADR-011's "every tool is annotated read-only". `tiles install opencode` writes `ask`, and `tiles install codex` adds the line `default_tools_approval_mode = "prompt"` under `[mcp_servers.tiles]` in Codex's `config.toml` after `codex mcp add`, which has no flag for it, keeping every other byte of the file. Since `codex mcp add` writes the server's table afresh and would drop that key, `tiles install codex` refuses to run when the key already holds a value other than `prompt`.
- An install made before Omni was configured keeps its no-Omni settings until something changes them, so `tiles omni set-key` re-applies the Omni settings to what Tiles installed: `ask` for the three tools in the `OpenCode` `config.json` Tiles owns, where `tiles_chart` is at Tiles' `allow` or a tool is unlisted, and `prompt` for a Codex tiles server with no approval mode. It edits no file Tiles does not own and no approval value the user set. With Omni configured, `tiles doctor` warns about an install that still runs a Tiles tool unasked and names `tiles install <harness>` as the fix. An install made without Omni keeps `tiles_chart` pre-approved.
- The Omni URL and key come both from the environment or both from the Keychain; a mix is refused. The Keychain key lives under account `api-key@<host>`, so a URL item rewritten to another host finds no key. This supersedes ADR-008's per-setting lookup.
- `tiles omni set-key <url>` reads the key with echo off and creates both Keychain items itself. An item a program creates trusts only that program, so any other process, `/usr/bin/security` included, gets a macOS prompt before reading it.

## Consequences

- With Omni configured, the user approves each chart in Claude Code, OpenCode and Codex, started through `tiles run` or installed with `tiles install`, unless they pass `--allow-omni` to `tiles run`.
- The model cannot narrate an Omni card's numbers by default; it can only point at the card. Refusals still name the model's own highlights and column names, so a model can learn whether a value it guessed exists in a topic, one call at a time.
- The environment route stays for machines without a Keychain. Every process the harness starts inherits `TILES_OMNI_API_KEY`, so `docs/omni.md` recommends the Keychain on a company machine. `tiles run` cannot remove the variable, because the MCP server the harness starts needs it.
- A new `tiles` binary has a new code signature, so the first read after an update shows the macOS prompt once.
- Pi itself has no tool approvals, so Omni tools run there without a prompt.
- Related: [ADR-005](ADR-005-privacy-and-secrecy.md), [ADR-008](ADR-008-data-sources.md), [ADR-009](ADR-009-security-baseline.md), [ADR-011](ADR-011-surfaces-v0-1.md).
