# ADR-056: The release job signs SHA256SUMS with an ssh key that `install.sh` pins and checks with macOS's own `ssh-keygen` and advisories are checked weekly

- Status: Accepted; amends [ADR-014](ADR-014-release-integrity.md)'s "No signature or build provenance yet" and [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md)'s releases token consequence, and adds a weekly run to [ADR-024](ADR-024-security-baseline-in-ci.md)'s dependency audits; the public repository signs with a new key ([ADR-059](ADR-059-public-cut.md))
- Date: 2026-10-22

## Context

A security review on 2026-10-19 (findings S2 and S3) found two gaps. At the time the code lived in a private repository on GitHub Free, which offers no branch protection or rulesets for a private repository; vulnerability alerts were off and nothing ran `cargo audit` between pull requests, so an advisory published on a quiet week waited for the next PR or push to main ([ADR-055](ADR-055-main-skips-a-tree-its-pull-request-passed.md) keeps `policy` on main for that reason). And `install.sh` is downloaded and run before anything checks it, while `SHA256SUMS` sits in the same release as the tarballs it covers, so whoever holds the releases repository's write token can replace a tarball, `SHA256SUMS` and `install.sh` together ([ADR-014](ADR-014-release-integrity.md), [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md)).

The review proposed signing `SHA256SUMS` in the publish job and having `install.sh` check the signature against the release workflow's identity. Apple code signing waits for a paying customer, and a Homebrew formula after the repository goes public becomes the main Mac route, so `install.sh` and the package stay the routes until then, and the check has to run on a Mac with nothing installed.

A stock Mac (checked on macOS 13.7.8) ships OpenSSH 9.0's `ssh-keygen`, which signs and checks with `-Y sign` and `-Y verify`, and LibreSSL 3.3.6's `openssl`. It has no cosign, `gh`, `jq` or Python. The four candidates:

| Method | Checks on a stock Mac | Secret | Why not |
|---|---|---|---|
| cosign keyless (Fulcio certificate for the workflow's OIDC identity) | Only partly, through a hand-written LibreSSL check of the certificate chain, its SAN and the signature | None | It cannot check the transparency log or timestamp, so it cannot show the signature was made inside the certificate's ten minutes; a Fulcio root rotation needs a new `install.sh`; a home-made Sigstore verifier risks failing open |
| GitHub artifact attestation | No: needs `gh attestation verify` | None | Offered on private repositories only with Enterprise Cloud |
| GitHub's OIDC token with the sums' hash as its audience | Only by downloading GitHub's key set at install time and building a PEM key in `sh` | None | Older releases stop verifying once GitHub rotates its signing key |
| ssh key in the release workflow's secrets | Yes, with `ssh-keygen -Y verify` | A long-lived private key | Chosen |

No method checks on a stock Mac without a long-lived secret, so the owner chose the ssh key on 2026-10-22.

## Decision

### Signed sums

- **The key.** One ed25519 key, comment `tiles-release`, signature namespace `tiles-release`. Its private half is the secret `TILES_SIGNING_KEY` of this repository's `release` environment ([ADR-059](ADR-059-public-cut.md)), created by the owner; no copy is kept on a development machine. Its public half, fingerprint `SHA256:PoJflLMy3GoOM5xsIUXgQFQUn8hCPW4GKA0DTpUNeDQ`, is the `signing_key` line of `scripts/install.sh` and is printed in [install.md](../install.md).
- **The release job.** `preflight` fails the release in seconds when `TILES_SIGNING_KEY` is missing. `publish`, which runs no dependency code, writes `SHA256SUMS` as before (the package, both tarballs and `install.sh`), writes the key to a file only the runner's user can read, runs `ssh-keygen -Y sign -n tiles-release` over `SHA256SUMS`, and deletes the key. It then checks the signature against the key `install.sh` pins and stops the release on a mismatch, so a wrong secret never ships a release no installer accepts. `SHA256SUMS.sig` is uploaded beside `SHA256SUMS`.
- **`install.sh`** downloads `SHA256SUMS.sig` with `SHA256SUMS`, writes an allowed-signers file from its pinned key, and runs `ssh-keygen -Y verify -I tiles-release -n tiles-release` before it reads a line of `SHA256SUMS`. A failed download of the signature, a signature by another key or for another namespace, a damaged one, or a `SHA256SUMS` changed after signing each print one line and install nothing: `tiles: cannot download <url>/SHA256SUMS.sig, the signature of SHA256SUMS, so its checksums cannot be trusted; not installed` or `tiles: SHA256SUMS is not signed by the Tiles release key; not installed`. It has no switch to skip the check, since a switch an installer can be told to set protects nothing.
- **A local install** with `TILES_ARCHIVE` and `TILES_SUMS` reads the signature from `$TILES_SUMS.sig`, the name a browser gives `SHA256SUMS.sig` downloaded beside `SHA256SUMS`. Without one it refuses, with the same check and no exception: `tiles: no signature <path> for <sums>; download SHA256SUMS.sig from the same release beside it; not installed`. A local install is how a person installs from a private release page, the route an attacker with the releases token would target, so it gets no weaker check.
- **The tag stays bound.** Downloading a tag, `install.sh` looks up `tiles-<tag>-<target>.tar.gz` in `SHA256SUMS`, and the signature covers those names, so a signed `SHA256SUMS` from an older release cannot pass for the tag asked for.
- **By hand.** [install.md](../install.md) gives the allowed-signers line, the `ssh-keygen -Y verify` command and `shasum -a 256 -c --ignore-missing SHA256SUMS`, which also covers the package and `install.sh`. It pins this `install.sh`'s own SHA-256, so a person can check the script before running it, the review's interim step, kept since the script is still run before it has checked anything.
- **Tests.** `scripts/test_install.sh` runs a copy of `install.sh` that pins a throwaway key: a good signature installs, from a local archive and from a release; a `SHA256SUMS` rewritten for another archive under the old signature, one signed by another key or for another namespace, a damaged signature, a missing one locally and a release without `SHA256SUMS.sig` each install nothing and print one line. It also fails when `ssh-keygen -l` does not read the key `install.sh` pins as this ed25519 key, by its fingerprint, or when `docs/install.md` lacks the pinned key or the script's current SHA-256, so a change to the script updates the page.

### Weekly advisories

- `.github/workflows/advisories.yml` runs `cargo deny check` and `cargo audit` with the `policy` job's pinned tools and `deny.toml` every Monday at 06:17 UTC and on `workflow_dispatch`. A failure is a failed scheduled run, which GitHub emails to the person who last changed the schedule.
- Dependabot vulnerability alerts are a repository setting, turned on by the owner (`gh api --method PUT repos/<owner>/<repo>/vulnerability-alerts`) and on for this repository; nothing in this repository sets it.

## Consequences

- Whoever holds the releases repository's write token alone can no longer replace a release's files in a way `install.sh` or the hand check accepts. Here the key is a secret of the `release` environment, which only `v*` tags deploy to ([ADR-059](ADR-059-public-cut.md)).
- A leaked key leaves no trace, since no transparency log records signatures. Replacing the key means a new secret, a new `signing_key` line and install.md, and every older `install.sh` then refuses new releases; a person downloads the new `install.sh` with the release, as they do today.
- Releases published before this change have no `SHA256SUMS.sig`, and the new `install.sh` refuses them; each still installs with the `install.sh` from its own release.
- The package route opens `tiles-<tag>-macos.pkg` without a check unless the person runs the hand check first; Gatekeeper's Open Anyway step does not check where it came from.
- `tiles update` still checks the archive against `SHA256SUMS` only, not `SHA256SUMS.sig`. It downloads through `gh`, so the same token holder could swap both; checking the signature there, through `/usr/bin/ssh-keygen`, is left for a later change, and install.md says so.
- Signing adds no dependency and no download to the release or the install: `ssh-keygen` is on GitHub's Ubuntu runner and on every Mac.
- Related: [ADR-009](ADR-009-security-baseline.md), [ADR-014](ADR-014-release-integrity.md), [ADR-022](ADR-022-sealed-assets-and-releases-only-channel.md), [ADR-024](ADR-024-security-baseline-in-ci.md), [ADR-035](ADR-035-macos-package-and-tiles-setup.md), [ADR-055](ADR-055-main-skips-a-tree-its-pull-request-passed.md).
