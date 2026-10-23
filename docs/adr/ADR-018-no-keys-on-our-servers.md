# ADR-018: Tiles never stores a user's keys on its servers, now or on any future platform

- Status: Accepted
- Date: 2026-10-18

## Context

On 2026-10-18 OpenCode reported that "a large scale distillation attack hit 1.4% of OpenCode Console users", and that those users' stored API keys were exposed. A hosted console that keeps its users' provider keys is one place an attacker can take many keys at once.

Tiles has no account, console or server today. The only key it holds is the Omni key, kept in the Mac's login Keychain, bound to its Omni host and readable without a prompt only by the `tiles` binary that created it ([ADR-013](ADR-013-data-stays-local.md)). Nothing leaves the machine except the user's own Omni query ([ADR-005](ADR-005-privacy-and-secrecy.md)), and the release that installs `tiles` is checked against hashes from the build jobs ([ADR-014](ADR-014-release-integrity.md)). A later paid platform with accounts would be the same kind of target as OpenCode's console.

## Decision

- No user secret is ever stored on a server Tiles runs. A user secret is any key, token or password for a system other than Tiles: an Omni key, a model provider key, a warehouse login.
- Those secrets stay on the user's machine (the macOS Keychain today) or in the customer's own secret store, read there by code running in the customer's environment.
- A Tiles account, when one exists, signs in through an identity provider or a credential Tiles keeps only as a hash. It never doubles as a store for the user's other keys.

## Consequences

- A breach of any future Tiles server exposes no Omni, provider or warehouse key, because none is there.
- Ruled out: a hosted console or dashboard that keeps a user's keys to query Omni or a model on their behalf, "sign in once and we hold your keys" convenience, and key backup or sync through Tiles' servers.
- Hosted dashboards cannot refresh from Omni on our servers. They refresh on the user's machine or inside the customer's own infrastructure, with the customer's secret store.
- Team sharing shares cards and intents, never keys. Each teammate queries Omni with their own key.
- A hosted chart-intelligence service ([ADR-019](ADR-019-copying-without-company-data.md)) needs no user secret: it authenticates the Tiles account and nothing else.
- Any future feature that would need a user secret server-side needs a new record that supersedes this one.
- Related: [ADR-008](ADR-008-data-sources.md), [ADR-009](ADR-009-security-baseline.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-014](ADR-014-release-integrity.md).
