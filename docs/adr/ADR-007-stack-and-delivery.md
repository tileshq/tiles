# ADR-007: One Rust workspace, CI gates on macOS and Linux, release builds for macOS arm64 and x86_64, one-command install

- Status: Accepted; its release workflow's jobs are superseded by [ADR-014](ADR-014-release-integrity.md); its install from the private repository and its unsigned binary are superseded by [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md); its layout's single network owner and the `policy` job's check of it are extended by [ADR-031](ADR-031-warehouse-sources.md) to `tiles-net`, `tiles-omni` and `tiles-warehouse` (the crate table is as of round 3; `README.md` lists the crates today); its CI on a push to `main` is amended by [ADR-055](ADR-055-main-skips-a-tree-its-pull-request-passed.md), which skips the heavy jobs on a tree a pull request's CI passed on
- Date: 2026-10-17

## Context

One founder builds Tiles with coding agents, overnight, on a 2017 dual-core Mac. The bar is an enterprise product's: decisions recorded, a definition of done where every point names its check, CI gates that fail before a mistake merges, and a review pipeline on every PR. Finished green work merges without waiting for the owner. The repository was private at the time, where GitHub billed macOS runner minutes at ten times the Linux rate.

## Decision

### Stack

| Layer | Choice | Why |
|---|---|---|
| Language | Rust 1.99.1, edition 2024, pinned in `rust-toolchain.toml` | One static binary, no runtime to install, skia-safe (ADR-002) |
| Lints | rustfmt; clippy `all` and `pedantic` with warnings as errors; `unsafe_code = "forbid"` | Agents get the same feedback as CI |
| Adapters | TypeScript only where a harness requires it (Pi extension, OpenCode plugin), each a thin caller of the `tiles` binary | The chart logic exists once, in Rust |

### Layout

| Path | Holds | Depends on |
|---|---|---|
| `crates/tiles-intent` | The chart intent types, validator and generated schema (ADR-003) | nothing of ours |
| `crates/tiles-engine` | Scene building and the Skia raster | `tiles-intent` (from round 2) |
| `crates/tiles-mcp` | The MCP server over stdio | `tiles-intent`, `tiles-engine` |
| `crates/tiles-cli` | The `tiles` binary: render, run (the wrapper), install | all of the above |
| `crates/tiles-omni` (round 3) | The Omni source, the only crate allowed a network client (ADR-005, ADR-008) | `tiles-intent` |
| `adapters/pi`, `adapters/opencode` (round 3) | Harness add-ons | the `tiles` binary |
| `docs/` | ADRs, the definition of done, the playbook | |
| `scripts/` | Policy checks CI runs | Python 3 standard library only |

### CI: the merge gates

Defined in `.github/workflows/ci.yml`; Actions are pinned to commit SHAs, the cargo registry, build output and the downloaded Skia binary are cached, and a newer push to the same branch cancels the older run.

| Job | Runs on | Fails the PR when |
|---|---|---|
| `rust` | Ubuntu and macOS (arm64) | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` or `cargo test` fails on either |
| `policy` | Ubuntu | cargo-deny finds a licence outside the allow-list (GPL, AGPL and LGPL are refused), a RustSec advisory, or a dependency from anywhere but crates.io; a definition-of-done point or playbook rule lacks a real check; a crate other than the Omni source reaches a network client; a policy script's own tests fail |

The review pipeline adds review and fixes on top; these gates are the floor. This Mac covers macOS x86_64 locally; build one thing at a time on it.

### Release and install

- A tag `v*` runs a release workflow (round 3) that builds `tiles` on GitHub's macOS arm64 and macOS Intel runners and uploads two tarballs and a `SHA256SUMS` file to the GitHub release.
- Install is one command on the user's Mac that downloads the right tarball with `gh release download` (the repository was private at the time, so the download needed a GitHub sign-in with access to it), checks the SHA-256 sum, puts `tiles` in `~/.local/bin`, and runs `tiles install` to register the MCP server and add-ons with Claude Code, Pi and OpenCode. Files fetched by `gh` carry no quarantine flag, so Gatekeeper does not block the unsigned binary; signing and notarization come with public distribution.

### Definition of done

Every PR meets every point in [`docs/definition-of-done.md`](../definition-of-done.md). The craft rules behind the engine are in [`docs/card-rules.md`](../card-rules.md).

## Consequences

- macOS minutes are the expensive part of CI, so macOS runs only the `rust` job, and the platform-independent policy checks run once, on Linux.
- If a user's Mac cannot sign in to GitHub with `gh`, the install needs another channel; round 3 checks this first.
- The first cold build downloads a 16 MB Skia archive and takes about 132 s on this Mac (ADR-002); later builds hit the cache.
