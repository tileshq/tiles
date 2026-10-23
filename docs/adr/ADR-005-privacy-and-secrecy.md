# ADR-005: v0.1 runs fully on the user's machine; only chart intelligence, evals and preference data will ever live on a server

- Status: Accepted; its open question on open-sourcing the client and its list of what the shape-only hosted service receives are superseded by [ADR-019](ADR-019-copying-without-company-data.md); its no-telemetry rule is extended to the add-ons and checked as [ADR-024](ADR-024-security-baseline-in-ci.md) records; its single network owner and its one outbound connection are extended by [ADR-031](ADR-031-warehouse-sources.md) to `tiles-net`, `tiles-omni` and `tiles-warehouse` and to BigQuery's two hosts; its "no update checks" is amended by [ADR-032](ADR-032-update-command-and-harness-versions.md), whose daily check sends one anonymous `GET` to `api.github.com`; its licence is superseded by [ADR-038](ADR-038-open-core-split.md) (FSL-1.1-ALv2)
- Date: 2026-10-17

## Context

Tiles will draw cards from company data in Omni, on the machine of the person who asked for the chart. That data must never leave that machine for anywhere Tiles controls, and must never reach a development machine. The owner also chose to keep all the moats: chart intelligence, eval data, preference data and per-company context.

Code shipped to a user's machine cannot be kept secret: under US trade-secret law, reverse engineering is a proper means of learning it (18 U.S.C. 1839(6)(B)). A licence can forbid reverse engineering as a contract term, but the design tokens, the form library and the local rules inside the binary can still be read. Warp drew the same line in 2026: it opened its client and kept its cloud agent platform closed.

## Decision

### v0.1: nothing leaves the machine

- No telemetry, crash reports, update checks, licence checks or hosted calls of any kind.
- The one outbound connection is to the Omni instance the user configures, over HTTPS, carrying the user's own query (ADR-008). Without that configuration Tiles opens no socket.
- Only the Omni source crate may depend on an HTTP client or networking crate. `scripts/check-network-owners.py` fails CI when any other crate reaches one through its normal dependencies without going through the Omni crate. The MCP server speaks stdio only and listens on no port.
- Rendered cards are written only to the per-user cache directory (owner-only permissions) and to a path the user names for PNG export.
- What the harness sends to its model provider is the harness's business, but Tiles keeps its own text result short: the title and the values shown on the card. Rows fetched from Omni go straight to the engine and are not returned to the model.

### Later: what stays server-side, and why

| Asset | Where it lives | Why |
|---|---|---|
| Chart-intelligence model and rules beyond the local defaults | Hosted service, "shape-only": it receives column types, cardinalities, ranges and rank order, never values | The only part that can be kept secret, and it improves with every call |
| Eval corpus and prompts | Our servers | They define "better than the base model" |
| Preference data (which charts people keep, re-ask, pin, share) | Our servers, opt-in | It cannot be copied from a screenshot |
| Per-company context (metric definitions, topic notes) | The customer's own store, or ours under contract | It decides whether the numbers are right |

The hosted service is never required: the local rules always produce a card, so offline use and companies that refuse the call still work. It ships behind its own ADR with its own privacy review.

### Licence

All rights reserved (`LICENSE`). Whether v1's client is later open-sourced is the owner's call ("we can decide where leverage is later"); this record treats everything in the binary as readable either way.

## Consequences

- The design system, the form library and the local chart rules are visible to anyone who takes the binary apart. Their value is in staying ahead, not in secrecy.
- Any future network call is a new ADR and a change to the network-owners allow-list in the same PR, where review sees it.
