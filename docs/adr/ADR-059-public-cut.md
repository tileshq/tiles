# ADR-059: This repository is cut from a private repository as one root commit, builds without the warehouse, and releases from its own tags with the design pack fetched from the private design repository

- Status: Accepted; carries out [ADR-038](ADR-038-open-core-split.md)'s fresh root commit; supersedes [ADR-038](ADR-038-open-core-split.md)'s and [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md)'s releases-only repository and [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md)'s release key
- Date: 2026-10-22

## Context

[ADR-038](ADR-038-open-core-split.md) splits Tiles into this public repository, a private design repository and a private paid one, and starts this repository from one root commit of a cleaned tree, since the private repository's history, pull request conversations and early releases hold material this tree does not. Before the cut, `crates/tiles-cli` depended on the paid warehouse crate unconditionally, and the release workflow sealed design pack 1 from the house tokens, which stay private, and published to a releases-only repository with a token.

## Decision

### The cut

A script in the private repository cut this tree. It took the paths that go public from that repository's committed tree, made the public-only edits (the records, docs and agent skills drop what names a private file, the warehouse or the releases-only repository), let cargo prune `Cargo.lock` and `fuzz/Cargo.lock` to the public crates without changing a version, and wrote the tree as one root commit. A scan then refused any secret, any private name, any colour of the house tokens, any binary but the plain goldens, the fuzz corpus, the three Inter fonts and the README's images, and any history beyond the one commit, and ran `cargo deny check`, `scripts/check-docs.py` and `scripts/check-network-owners.py` inside the tree. The private repository's CI cut, scanned, built and tested this tree on every change before the first push.

### What stays out of this tree

The house `tokens.json`, `docs/playbook.md`, the house goldens, the prototype references with `tests/reference.rs`, `tools/`, `crates/tiles-design`, `crates/tiles-warehouse` with its fixtures, its docs and three of its fuzz targets (the `warehouse_source` target reads `tiles-intent` and stays), `scripts/test-house.sh` and the `house` CI job. The rule tests and one MCP test read seven gallery intents from `crates/tiles-engine/tests/intents/gallery/`, with the stacked share card's payment methods reworded as sign-up channels and its numbers kept.

`tiles-cli` builds with no warehouse: `tiles mcp`, `tiles doctor`, `tiles setup`, `tiles run` and `tiles check` say nothing about one, and `check-network-owners.py` names only the owners the workspace holds. The intent schema keeps its `data.warehouse` branch, the seam `tiles-pro` serves ([ADR-031](ADR-031-warehouse-sources.md)); here such a call is refused as no warehouse configured.

### The release

- **The design pack.** The private design repository `tileshq/tiles-design` holds the house tokens, the playbook, the house goldens, the prototype references, the `tiles-design` pack tool and the pack itself, and checks that `packs/tiles-design-1.pack` reproduces byte for byte from `tokens.json` ([ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md)).
- **`release.yml`.** Its build job fetches `packs/tiles-design-1.pack` from the design repository at the commit `DESIGN_COMMIT` names, over SSH with the repository's read-only deploy key and GitHub's host key pinned, in one step whose env alone holds the key and which deletes the key file before it ends; that step runs before any cargo command, so no dependency's build script has run while the key exists. It checks the pack against `scripts/design-pack.sha256`; `cargo test` then runs without `TILES_DESIGN_PACK`, on the plain set, and the build and binary-check steps take the pack's path through their own env. The sign job stays, unsigned until the Apple secrets exist. The publish job creates the release on this repository with its own `GITHUB_TOKEN` and `contents: write` on that job only; `RELEASES_REPO` is `${{ github.repository }}`. `preflight` checks `TILES_SIGNING_KEY` and `TILES_DESIGN_DEPLOY_KEY`, both secrets of a `release` environment that only `v*` tags may deploy to, and every job that reads a secret runs in it.
- **A new release key.** [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md)'s key stays a secret of the private repository, so this repository signs with a new ed25519 key. Its private half is `TILES_SIGNING_KEY` in the `release` environment, with no copy kept on disk; `install.sh` pins its public half, and [docs/install.md](../install.md) prints it with its fingerprint.
- **The public locations.** `update.rs`'s `RELEASES_REPO`, `install.sh`'s default repository, `SECURITY.md`'s update-check URL, `docs/install.md` and the release notes name `tileshq/tiles`; `TILES_RELEASES_REPO` still points them elsewhere.
- **Piped install.** The README installs with `curl -fsSL https://github.com/tileshq/tiles/releases/latest/download/install.sh | sh`. Piped, the script's stdin is the pipe, so when stdout is a terminal and `/dev/tty` opens it runs `tiles setup < /dev/tty`, the usual installer pattern and [ADR-054](ADR-054-dev-tty-only-beside-a-terminal.md)'s rule; with no terminal it prints the next step as before. `scripts/test_install.sh` runs the script piped into `sh` under `script` and with no terminal at all.
- **The repository's settings.** A ruleset on `main` requires a pull request and every check `ci.yml` runs on one, and blocks force pushes and deletions, with repository admins able to bypass; a tag ruleset lets only admins create, update or delete `v*` tags; the `release` environment holds the two secrets and admits only `v*` tags. Private vulnerability reporting and Dependabot alerts are on; pull requests merge by squash only, with the pull request's title as subject and its body as message.

## Consequences

- The private repository keeps the full history as the archive and is never made public.
- A new design pack takes a new commit in the design repository, a new `DESIGN_COMMIT` in `release.yml` and a new line in `scripts/design-pack.sha256`.
- This repository's releases verify only with an `install.sh` that pins the new key; releases made before the cut verify with ADR-056's key.
- Related: [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md), [ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md), [ADR-037](ADR-037-plain-theme-for-builds-from-source.md), [ADR-038](ADR-038-open-core-split.md), [ADR-040](ADR-040-licence-cla-and-trademark-files.md), [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md).
