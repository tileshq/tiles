# ADR-019: Tiles is closed source, guards its chart intelligence in layers, and never moves company data off the machine to do it

- Status: Accepted; its closed source and its "no published source" layer are superseded by [ADR-038](ADR-038-open-core-split.md), which publishes the code under FSL-1.1-ALv2 and keeps the house design pack private; its list of the layers that hold today is updated by [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md)
- Date: 2026-10-18

## Context

The attack OpenCode reported on 2026-10-18 was described as a large scale distillation attack. Distillation means calling a product thousands of times and training on its answers to copy how it thinks. Tiles' chart intelligence (the form library, the design tokens and the local chart rules) ships inside the binary today, where anyone who takes it apart can read it ([ADR-005](ADR-005-privacy-and-secrecy.md)). A product that wants to keep that logic secret is tempted to move it to a server and send company data there, which trades the user's data for our IP.

The owner, 2026-10-18: "we want it 1. secure and private to the point no one can copy our chart intelligence ... or copy our engine or anything so this novel tech remains ours. 2. we want to protect ourselves and our users and their data and their personal data against all types of attacks."

ADR-005 left open whether the client is later open-sourced, and plans a "shape-only" hosted service whose inputs include ranges and rank order, which are read from the values themselves.

## Decision

- Tiles is proprietary and closed source (`LICENSE`). It has no public repository, and no part of the engine, the design system or the chart rules is published. This supersedes ADR-005's open question on open-sourcing the client.
- Copy protection is layered, because nothing that runs on a user's machine is uncopyable in principle. The layers that hold today are: no published source; installs from a repository that holds only release files, so installers receive binaries and no source; the forms and design tokens sealed inside the binary and opened only in memory, with no source path left in a release build; a licence that grants no right to use, copy, modify, reverse engineer or decrypt any part of Tiles; and the eval corpus and preference data kept off the binary on our servers (ADR-005), since a copied binary carries neither ([ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md)).
- Company data and row values never leave the user's machine to protect Tiles' IP. Copy protection is never a reason to upload data.
- If any part of Tiles runs as a hosted service, it sees only chart shape and field names: the form asked for, field names, column types, and row and distinct-value counts. It sees no row value, label value, range, rank order or sample. It is rate-limited per account, so copying it in bulk takes many accounts and much time, and it has no anonymous access. This supersedes ADR-005's list of what the shape-only service receives.
- Chart picking stays fully local in the binary now. When the paid platform is introduced, it moves to the shape-only hosted service above. The owner, 2026-10-18: "yes lets go with A now and when we introduce the platform lets then go with B".

## Consequences

- `strings` and a JSON scan no longer read the chart rules from the binary, but someone who disassembles it can: the key that opens the sealed forms and tokens is derived inside the binary (ADR-022). The licence makes copying unlawful; it does not make copying impossible.
- Once chart picking moves to the hosted service with the paid platform, distilling it yields chart choices for shapes and field names, never company numbers, because the service never sees them. Field names then reach our servers; a company that refuses that keeps the local rules, which always produce a card (ADR-005).
- Ruled out: publishing the source or any part of it, uploading rows or sample values for "better charts", sending ranges or rank order to any Tiles server, and any telemetry that carries values.
- Accounts exist for any hosted service at least for rate limits; they hold no user secret ([ADR-018](ADR-018-no-keys-on-our-servers.md)).
- Hosted dashboards and team sharing that would carry values off the machine are features with their own record and privacy review, and are never justified as copy protection.
- A hosted chart-intelligence service ships behind its own record, as ADR-005 requires, and that record must meet this one.
- Related: [ADR-005](ADR-005-privacy-and-secrecy.md), [ADR-009](ADR-009-security-baseline.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-018](ADR-018-no-keys-on-our-servers.md), [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md).
