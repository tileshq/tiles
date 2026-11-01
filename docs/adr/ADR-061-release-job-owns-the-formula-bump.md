# ADR-061: The release job pins the Homebrew formula to each final release, from its signed sums, with a deploy key whose value only one step reads

- Status: Accepted; brings back the tap [ADR-035](ADR-035-macos-package-and-tiles-setup.md) says the open-core plan dropped; adds a third secret to the `release` environment [ADR-059](ADR-059-public-cut.md) describes
- Date: 2026-11-01

## Context

`brew install tileshq/tap/tiles` reads `Formula/tiles.rb` in the public repository [tileshq/homebrew-tap](https://github.com/tileshq/homebrew-tap). The formula pins one tarball url and one SHA-256 per architecture, so brew users stay on the pinned version until someone edits that file. Each release would need a hand edit, and a mistyped hash breaks `brew install` for everyone on that architecture. Pushing to the tap from this repository's release workflow takes a credential that can write to another repository.

## Decision

- **The job.** `bump-formula` is the last job of `.github/workflows/release.yml` and needs `publish`. It holds `contents: read` and runs no dependency code.
- **What it trusts.** It downloads `SHA256SUMS` and `SHA256SUMS.sig` from the published release, refuses a draft or a prerelease, and checks the signature against the key `scripts/install.sh` pins at the tag ([ADR-056](ADR-056-signed-sums-and-weekly-advisories.md)). The formula's hashes come from those sums. The job never downloads a tarball or computes a hash.
- **The rewrite.** `scripts/bump-formula.py` changes the version in both urls and in the formula's test block, and both `sha256` values. It refuses a tag that is not `vMAJOR.MINOR.PATCH`, sums without exactly one line per tarball, and a formula of another shape, each with a failed job and nothing written. It runs once on the sums before the key exists and again on the formula.
- **Re-runs.** A formula already pinned to this release is left alone and the job succeeds with no commit. A formula at this version with other hashes, or at a newer version, fails the job, since pushing would either hide a replaced release or move brew users backwards.
- **The commit.** One commit on the tap's `main`, as `github-actions[bot]`, with the message `tiles <version>`.
- **The key.** `TAP_DEPLOY_KEY` is the private half of a read-write deploy key on the tap titled `tiles release: bump formula`. It lives only as a secret of this repository's `release` environment, which only `v*` tags may deploy to. One step's env reads its value and writes it with mode 600 into the runner's temp directory, a later step uses the file with GitHub's host key pinned, and a last step deletes it, also after a failure. `preflight` sees only whether the secret is set, as a true or false, and stops the release in seconds when it is not; no other job or step sees the value.

## Consequences

- A final release reaches brew users with no hand edit once the release job ends.
- A failure in `bump-formula` leaves the release published and the formula on the older version. Re-running the job is safe. `preflight` catches a missing key before the macOS builds, but not a wrong one or one removed from the tap, which fails here at the clone.
- A prerelease tag publishes its release and then fails this job, which is the signal that the formula did not move. The formula stays on the last final release.
- A deploy key covers the whole tap, so whoever can run a workflow from a `v*` tag can write anything to the tap. The tag ruleset ([ADR-059](ADR-059-public-cut.md)) limits `v*` tags to admins. Replacing the key means a new deploy key on the tap and a new secret here, with no change to this repository's files.
- The tap's `main` takes direct pushes today. A ruleset that requires a pull request there would fail this job unless deploy keys may bypass it.
- A new formula shape in the tap fails the job until `scripts/bump-formula.py` and `fixtures/homebrew/tiles.rb` follow it.
- `tiles update` and the daily notice do not know about Homebrew yet: the notice says to run `tiles update`, which writes a second copy to `~/.local/bin`. [install.md](../install.md) tells brew users to run `brew upgrade tiles`.
- Related: [ADR-014](ADR-014-release-integrity.md), [ADR-035](ADR-035-macos-package-and-tiles-setup.md), [ADR-056](ADR-056-signed-sums-and-weekly-advisories.md), [ADR-059](ADR-059-public-cut.md).
