# ADR-009: Security baseline: treat every intent, row and label as hostile, write nothing raw to the terminal, keep credentials out of every output, and pin the supply chain

- Status: Accepted; its Skia download for release builds is superseded by [ADR-014](ADR-014-release-integrity.md); its property tests, fuzz target, fuzzing budget in CI and `SECURITY.md` are delivered by [ADR-024](ADR-024-security-baseline-in-ci.md)
- Date: 2026-10-17

## Context

The owner asked for "full security review too". Tiles runs on a company machine inside an AI coding session, reads company data, holds an Omni credential and writes escape sequences into the user's terminal. Its inputs come from a model, which can be steered by anything in its context (a README, an issue, a data row), so a model-written intent is untrusted input. A security baseline written for an earlier product (OWASP ASVS mapping, supply-chain rules after the 2025 npm compromises) sets the standard; this record applies it to a local binary.

## Decision

### Threat model

| Asset | Threat | Control |
|---|---|---|
| The terminal session | A data label or title carrying escape sequences: clipboard writes (OSC 52), fake hyperlinks (OSC 8), title changes, cursor moves that hide or forge output, bidi overrides that reorder text | Labels and titles reach the user as pixels in the image, never as terminal text. All text Tiles prints (alt text, errors, the wrapper's own lines) goes through one sanitizer that removes C0 and C1 controls, ESC and bidi controls. All graphics escapes come from one encoder that writes only its own header and base64 payload. A test feeds a hostile-label corpus through every output path |
| The process | Malformed or oversized intents and rows: deep nesting, huge strings, NaN, millions of rows, an unknown form | Strict parsing with unknown keys refused (ADR-003); limits on rows (10,000), string lengths, nesting and total payload (2 MB); non-finite numbers refused; no panics on any input, checked by property tests now and a fuzz target before stage 2. No intent field is ever evaluated as code: no expressions in v0.1, and the later Vega bridge runs sandboxed |
| Company data | Rows leaving the machine, or reaching the model when they need not | ADR-005 and ADR-008: one network crate, rows to the engine only, a short text result |
| The Omni credential | Leaking through logs, errors, tool results, crash output or the intent | ADR-008: env or Keychain only, redacted `Debug`, no `Display`, a planted-key test over every output |
| The user's files | PNG export writing outside the path the user named, or through a symlink planted in the cache | Export writes to a temporary file in the target directory and renames it into place; the cache directory is created owner-only and Tiles refuses it if another user owns it |
| The binary | A compromised or invented dependency, a tampered Skia download, a tampered release | `Cargo.lock` committed; crates.io only, no git dependencies; cargo-deny for licences and RustSec advisories in CI; a new dependency needs a reason in the PR; Actions pinned to commit SHAs; release binaries built only in CI with SHA-256 sums the installer checks (ADR-007) |
| Local reach | Another process on the machine talking to Tiles | The MCP server speaks stdio only and listens on no port. The wrapper uses a pseudo-terminal, not a socket |

The Skia download at build time comes from rust-skia's GitHub releases over HTTPS for the pinned version. It runs on developer machines and in CI, never on a user's machine.

### Review on every surface

Every PR that adds or changes a surface (the MCP tools, a CLI command, the wrapper, a harness add-on, a data source) fills the Security line of the PR template's checklist against the table above: which rows apply, how each is met, what was not verified. The full review of v0.1 (the security review) walks every row against the shipped code before the owner uses it with work data, and adds a missing test or gate for each finding.

## Consequences

- Every new output path, input key or data source carries the cost of a limit, a sanitizer path and a test.
- The hostile corpus and the planted-key test become shared fixtures that each surface reuses.
- Before a public release: signing and notarization, a fuzzing budget in CI, and a `SECURITY.md` with a reporting address.

Sources: that earlier product's security-baseline record (private); https://invisible-island.net/xterm/ctlseqs/ctlseqs.html (OSC 8 and OSC 52); https://github.com/EmbarkStudios/cargo-deny (fetched 2026-10-17).
