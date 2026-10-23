# ADR-001: Tiles ships in five stages, and v0.1 is simple charts in the terminal, fully local

- Status: Accepted; stage 2's open question on open-sourcing the client is superseded by [ADR-019](ADR-019-copying-without-company-data.md); its "update checks" non-goal and its one outbound connection are amended by [ADR-032](ADR-032-update-command-and-harness-versions.md), whose daily check sends one anonymous `GET` to `api.github.com`; its "companion window" non-goal is amended by [ADR-046](ADR-046-companion-window-on-the-text-route.md), which adds one on the text route on macOS
- Date: 2026-10-17

## Context

Tiles draws gallery-quality chart cards inside the terminal for AI coding tools. A prototype (MCP tool, HTML card drawn by headless Chrome, a `tiles run` wrapper) proved the look in nine rounds of testing. The research review of 2026-10-17 concluded that a product needs a native engine, a short chart intent instead of a model-written spec, official harness surfaces, and a moat in chart intelligence, eval data, preference data and per-company context.

The owner set the order on 2026-10-17: terminal cards first; then cards with filters, shown publicly in LinkedIn posts; then full-page dashboards in the terminal; then a shareable link or an engine window; then a paid, AI-driven analytics platform, "... fully paid. So we intro things slowly." The bar is an enterprise product's: "not vibe coded but a full enterprise level AI analytics product". At 22:10 he added Pi and OpenCode next to Claude Code, Omni topics as a data source, "simple charts are good enough no filters needed but lets add as many charts as we can over night", and "fully tested to the top standard, full security review too." At 22:20: "this should also run on normal terminal not just warp, opencode, pi, codex."

## Decision

### Release order

| Stage | What ships | Who sees it |
|---|---|---|
| 1. Tiles in the terminal (v0.1) | One chart card per request, inline in the terminal | The owner, daily |
| 2. Tiles with filters | Cards that take filters; shown publicly in LinkedIn posts | Public, as posts. Whether v1's client is open-sourced stays open (ADR-005) |
| 3. Full-page dashboards in the terminal | Several cards laid out to fill a terminal page | Early users |
| 4. A shareable link or an engine window | A dashboard that lives outside the transcript: a link, or our own window beside the terminal | Teams |
| 5. AI-driven analytics platform | Governed company data, chart intelligence, dashboards, paid | Customers |

A stage starts only when the one before it works daily for the owner. Each stage gets its own ADR before work starts.

### v0.1 scope

- **Charts:** the simple forms the engine round can land, as many as pass the golden tests (ADR-006). No filters, no interactivity beyond what the harness gives.
- **Engine:** native Rust and Skia, no browser at run time (ADR-002), driven by the chart intent (ADR-003).
- **Surfaces:** Pi, OpenCode, Claude Code and Codex, in Warp, in Warp inside herdr, and in ordinary terminals such as Terminal.app, with a defined fallback where a terminal shows no images (ADR-004).
- **Data:** inline rows from the model, and Omni topics read with the user's own key (ADR-008).
- **Output:** the card inline in the terminal, and PNG export to a path the user names.
- **Privacy:** runs fully on the user's machine. Tiles sends nothing to us or any third party; its one outbound connection is to the Omni instance the user configures (ADR-005).
- **Install:** one command on macOS arm64 and x86_64 (ADR-007).

### v0.1 non-goals

Filters; dashboards; links and sharing; the companion window; the MCP App; hosted chart intelligence; accounts, billing, telemetry or update checks; Windows and Linux release builds (CI still tests Linux); Grok.

## Consequences

- v0.1 is judged by one thing: the owner makes a chart from real data every day and posts it.
- Anything outside the scope above needs a new ADR, not a quiet addition.
- Stage 2's public LinkedIn posts make the card look visible to everyone, so the look cannot be the moat (ADR-005).
