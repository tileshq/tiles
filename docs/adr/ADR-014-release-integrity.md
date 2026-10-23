# ADR-014: Release builds hold no write token, SHA256SUMS comes from the build jobs, and the Skia archive is checked against a pinned hash

- Status: Accepted; its publish job now writes to the releases-only repository, after a signing job, per [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md); its "No signature or build provenance yet" is amended by [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md), whose publish job signs SHA256SUMS with a key `install.sh` pins
- Date: 2026-10-18

## Context

The security review of `main` at `776d462` (finding F5) found three gaps in `.github/workflows/release.yml`. The build jobs held `contents: write` with the checkout's credentials kept on disk while every dependency's build script and proc macro ran. The `sums` job downloaded the tarballs back from the release and hashed whatever the release then held, so a party able to write to the release could swap a tarball and SHA256SUMS together. And skia-bindings 0.153.3 fetches the prebuilt Skia archive linked into `tiles` and unpacks it with `// TODO: verify key` (`build_support/binary_cache/download.rs`); `Cargo.lock` pins the crate, not the archive.

## Decision

- The build jobs run with `contents: read` and `persist-credentials: false`. Each hashes the tarball it built and hands the tarball and its hash to a `publish` job as an artifact.
- `publish` alone holds `contents: write`. It checks every tarball against its build job's hash, writes SHA256SUMS from those hashes, and creates the release with the tarballs and SHA256SUMS in one step, so a failed build leaves no release.
- `scripts/skia-binaries.sha256` pins the SHA-256 of the Skia archive for each target, taken from the digests GitHub publishes for rust-skia's `0.153.3` release (published 2026-09-04) and matched against the archive a local build had downloaded. `scripts/fetch-skia.sh` downloads the archive for the build's target, refuses one whose hash differs or that is not pinned for the skia-bindings version in `Cargo.lock`, and sets `SKIA_BINARIES_URL` to the checked file, which skia-bindings reads through its `file://` support (`build_support/binary_cache/env.rs`). This supersedes ADR-009's "comes from rust-skia's GitHub releases over HTTPS for the pinned version" for release builds.

## Consequences

- A skia-safe upgrade must add the new archives' digests to `scripts/skia-binaries.sha256`, or the release build stops at the fetch step. A skia-safe feature change must replace them too, since `fetch-skia.sh` hands skia-bindings the one pinned file for the target, whatever features the build asks for.
- CI builds still let skia-bindings download the archive unverified; they ship nothing.
- No signature or build provenance yet: SHA256SUMS still lives in the same release as the tarballs, so it guards against a corrupted or swapped upload made outside the publish job, not against the publish job itself. Provenance attestation waits for the public release (ADR-009).
- Related: [ADR-007](ADR-007-stack-and-delivery.md), [ADR-009](ADR-009-security-baseline.md).
