# ADR-024: CI audits dependencies, fuzzes every input parser and scans for secrets; Tiles sends no telemetry and collects no personal data, and a test holds it

- Status: Accepted; its network owners and documented hosts are extended by [ADR-031](ADR-031-warehouse-sources.md) (`tiles-net`, `tiles-warehouse`, `bigquery.googleapis.com`, `oauth2.googleapis.com`); its "no update check" is amended by [ADR-032](ADR-032-update-command-and-harness-versions.md), whose daily check sends one anonymous `GET` to `api.github.com`, named only by the update code; F6's refusal of every format character in model text is amended by [ADR-050](ADR-050-macos-system-fonts-behind-inter.md), which accepts a zero width joiner between two emoji; its dependency audits also run every week, per [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md); the add-ons' tests and fixture cards also run with no network under a trace that fails on any try to reach one, per [ADR-058](ADR-058-add-ons-and-fixture-cards-run-with-no-network.md), so a disguised destination in the paths that job runs no longer rests on review alone
- Date: 2026-10-18

## Context

The owner, 2026-10-18: "we want it 1. secure and private to the point no one can copy our chart intelligence ... or copy our engine or anything so this novel tech remains ours. 2. we want to protect ourselves and our users and their data and their personal data against all types of attacks."

[ADR-009](ADR-009-security-baseline.md) promised "property tests now and a fuzz target before stage 2", and, before a public release, "a fuzzing budget in CI, and a `SECURITY.md` with a reporting address". The v0.1 security review closed its high and medium findings F1 to F5 in [ADR-013](ADR-013-data-stays-local.md) and [ADR-014](ADR-014-release-integrity.md), and copy protection landed in [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md). Its low and informational findings F6 to F14 were open, CI ran `cargo deny` but no second advisory reader and no secret scan, and [ADR-005](ADR-005-privacy-and-secrecy.md)'s "no telemetry" was checked only for which crate links a network library.

## Decision

### Dependency audits

The `policy` job runs `cargo deny check` (advisories, licences, bans and sources, from the existing `deny.toml`) and `cargo audit` 0.22.2, which reads the same RustSec database over every package in `Cargo.lock` whatever its target. Either fails the job on a known vulnerability; `cargo deny` also fails on a disallowed licence, a yanked or unmaintained crate, a wildcard version or a source other than crates.io.

### Fuzzing

`fuzz/` is a cargo-fuzz project in its own workspace, so the main build never compiles it. Four targets cover every parser of untrusted input:

| Target | Input | Holds |
|---|---|---|
| `terminal_stream` | The harness's output through the wrapper's painter on every route, with resizes and clock ticks | No panic escapes; each chunk comes out whole and in order, the painter's own bytes only where `Painter::feed_painted` puts them: before the harness's synchronized-update end, around the bytes at ground in an update of its own (ADR-030), and before an unfinished sequence |
| `terminal_replies` | The terminal's answers to Tiles' queries (`Replies::parse`) | No panic; the version text is safe to print |
| `intent` | A chart intent (`parse_request`, `into_intent`) | No panic; a refusal's text is safe to print |
| `block` | Placeholder tokens on a screen row (`block::find`, `block::partial`) and the lines Tiles prints for a title | No panic; tokens are valid; printed lines are safe |

The painter lives in the `tiles` binary crate, so `terminal_stream` compiles `crates/tiles-cli/src/wrap/paint.rs` and `escape.rs` themselves through `#[path]`; it fuzzes the shipped code, not a copy. Those two files may therefore reach only each other, `std` and library crates such as `tiles_term`: a helper the painter needs lives in `paint.rs` (`conversation_cols`) or a library crate (`tiles_term::image::png_size`). Only a build of the fuzz workspace (`cd fuzz && cargo build --release --bins`) catches a reach into the binary crate; the main workspace's build does not. Each target has a small committed seed corpus in `fuzz/corpus/<target>/`. The `fuzz` CI job runs each for 60 seconds under AddressSanitizer on `nightly-2026-10-01` with cargo-fuzz 0.13.2, fails on a crash, and keeps the crashing input as an artifact. `fuzz/README.md` says how to run one locally.

### Property tests

ADR-009's property tests now cover every parser and the engine. The sanitizer, the block parser and the reply parser (`crates/tiles-term/tests/property.rs`) and the painter (`crates/tiles-cli/src/wrap/paint/tests.rs`) came with the round that made ADR-013 (pull request 6 of the pre-cut private history). This record adds the intent parser (`crates/tiles-intent/tests/property.rs`: 25,000 mutated and random intents) and the engine (`crates/tiles-engine/tests/property.rs`: every form's fixture mutated 80 ways, each drawn or refused, with every mark at finite coordinates and no unsafe character in any drawn text). They use a seeded xorshift generator, so they add no dependency and a failure names its seed.

### Secret scanning

The `secrets` CI job runs gitleaks 8.30.1 with its default rules over every commit and over the tree, and fails on a finding. `scripts/fetch-gitleaks.sh` downloads gitleaks and refuses a tarball whose SHA-256 differs from the one pinned for that platform. `.gitleaks.toml` allows exactly one finding: the throwaway P-256 key the Omni replay tests serve over local TLS, matched by file, rule and value, so a second key in the same file still fails.

### No telemetry, no personal data

Tiles sends no telemetry, crash report, analytics, update check or licence check, and collects no personal data. At run time the `tiles` binary connects only to the Omni host the user configures, through `tiles-omni`; the harness add-ons connect to nothing and run only the `tiles` binary. `scripts/check-network-owners.py` runs on every PR. Two of its checks are the evidence, because they read cargo's resolved dependency graph (`cargo metadata`), not source text: only `tiles-omni` reaches a networking crate at run time, and no crate, `tiles-omni` included, reaches a telemetry, crash-reporting or analytics crate. Its other three checks scan source text and are a lint: no other crate names a socket type; no shipped Rust source names an http(s) URL whose host is not an Omni host or a placeholder for the configured one; and no add-on file names a URL or a network API (`fetch`, `node:http`, `node:net`, `WebSocket` and the like), imports `child_process` other than by name, or spawns anything but `tiles`. The lint catches the plain ways to add a destination; code written to hide one (a host built at run time, `globalThis["fe" + "tch"]`) passes it, and review is what stops that. A new destination needs a new record and a change to that script. `SECURITY.md` states the rule for users.

### The review's low and informational findings

- **F6.** One predicate, `tiles_intent::text::is_unsafe_char`, now refuses on input and strips on output every format character (Unicode category Cf: zero-width characters, the byte order mark, the soft hyphen, bidi controls and the tag characters) as well as controls and the line and paragraph separators. Text from the model and printed text share it, and the Pi and Claude Code add-ons' cleaners drop every `\p{Cf}` character. A refusal calls a control character "control" and any other "invisible". Omni cells come from the warehouse, not the model, and a zero width non-joiner spells many Persian and Indic words, so an Omni text cell loses its format characters (`text::strip_format`) and the chart still draws; a zero width joiner inside an emoji sequence such as 👩‍💻 stays, and a control character or line separator still refuses the query.
- **F7.** The property tests above, and the fuzz targets.
- **F8.** A number cell larger than 1e15 in magnitude is refused with an error naming its row and column, for inline rows and Omni rows alike, so no stack, running total or label overflows.
- **F9.** `tiles run` now hands its session token, `TILES_SESSION`, to every harness the wrapper runs, on every route, not only where it plays the entrance in kitty (superseding that part of [ADR-017](ADR-017-early-card-and-entrance.md)). The wrapper draws a card only when its geometry names that token, so a block naming a card from an earlier session, another `tiles run`, or `tiles card` in another shell stays text, on Warp and sixel as in kitty. Sending cards ahead stays a kitty feature, and a theme change still swaps cards in place only where the wrapper plays entrances (`TILES_FRAMES` is set too), because Warp and sixel cannot change a picture already on screen. A new card is linked into the cache under a fresh id and never replaces a file, so an old block in scrollback cannot repaint with a different chart. `tiles open <id>` inside the session draws an earlier card again as this session's.
- **F10 and F11** were closed in that same round: a lone C1 byte no longer stalls drawing, and a cache PNG larger than 16,384 pixels a side or 2^26 pixels is refused before decoding.
- **F12.** The Claude Code mod reads the last `PNG:` line of a result and accepts it only when it names `.../cards/<id>.png` for the card id in that result.
- **F13.** ADR-022's `sign` job signs and notarises the macOS binary once the owner adds the six Apple secrets; nothing is left in code. Until then releases ship unsigned and `docs/install.md` keeps the `xattr` step for a browser download.
- **F14.** Tiles overwrites the copies of the Omni key it makes itself: `ApiKey`'s buffer, the text read from the Keychain, the environment or the terminal (also when the configuration is refused), and the `Authorization` header value. The process environment, the Keychain framework's buffers, standard input's read buffer and the HTTP client's own header copy are not, and the docs claim no more.

### SECURITY.md

`SECURITY.md` says how to report a vulnerability privately (email to the owner; GitHub's private vulnerability reporting once a repository is public, since GitHub offers it on public repositories only), which versions get fixes (the latest release), and the no-telemetry rule.

## Consequences

- CI gains two Linux jobs: `fuzz` (about 4 minutes of fuzzing plus a cached build) and `secrets` (under a minute). The nightly date, cargo-fuzz, cargo-audit and gitleaks versions are pinned and bumped by PR; a gitleaks bump needs its digests checked against downloads.
- A crash the fuzzer finds fails the PR that exposes it, even when the code it reaches did not change; the fix adds the crashing input to the seed corpus.
- Cards from an earlier `tiles run` session, or made outside this one, no longer draw over their blocks when a conversation shows them again.
- An Omni text cell with a zero-width or other format character draws without it.
- An intent whose values exceed 1e15 must be sent in a larger unit.
- This meets ADR-009's fuzz target, fuzzing budget and `SECURITY.md`; signing and notarisation stay with ADR-022.

Sources: https://github.com/rust-fuzz/cargo-fuzz (0.13.2, `src/project.rs`, read 2026-10-18); https://github.com/rustsec/rustsec/tree/main/cargo-audit (0.22.2); https://github.com/gitleaks/gitleaks (v8.30.1 release checksums, fetched 2026-10-18); https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository.
