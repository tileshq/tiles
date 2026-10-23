# Contributing to Tiles

Tiles is source-available under the Functional Source License (FSL-1.1-ALv2, in [LICENSE](LICENSE)). Bug reports, harness and terminal requests, chart form requests and pull requests are welcome. Report a security problem privately, as [SECURITY.md](SECURITY.md) says, never in an issue.

Before a large change, open an issue first so the approach can be agreed before you build it.

## The contributor licence agreement

Every contributor accepts the [Tiles Individual Contributor Licence Agreement](.github/CLA.md) once, on their first pull request. A bot comments on that pull request with a link; you sign in with GitHub, accept, and the check turns green. Later pull requests are not asked again.

The agreement is modelled on the Apache Individual CLA. You keep the copyright in your work and grant a copyright and patent licence, with the right to relicense. Tiles needs that right because your code ships in three places: the public source under FSL, the official binary beside the house design pack under its own licence ([LICENSE-design](LICENSE-design)), and the paid build. A Developer Certificate of Origin sign-off alone grants neither licence.

The bot is [CLA Assistant](https://cla-assistant.io), free and hosted. It needs no workflow in this repository and no secret. It shows the agreement from a gist the maintainer keeps identical to `.github/CLA.md`, and a change to the agreement changes both.

## Build and check

The [definition of done](docs/definition-of-done.md) lists what every pull request meets, and each point names its check. Run what CI runs (`.github/workflows/ci.yml`) before you open a pull request:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`
- `cargo deny check` and `cargo audit`
- `python3 scripts/check-docs.py`, `python3 scripts/check-network-owners.py` and `python3 -m unittest scripts/test_policy_scripts.py`
- `node --test adapters/pi/index.test.ts adapters/claude/card.test.ts`
- `sh scripts/test_install.sh` and `sh scripts/test_package.sh`
- `scripts/check-release-binary.py` on a release build made with `RUSTFLAGS="$(sh scripts/release-rustflags.sh)"`
- gitleaks, fetched by `scripts/fetch-gitleaks.sh`
- the fuzz targets ([fuzz/README.md](fuzz/README.md))
- the formal proofs, `sh verify/formal/painter/check.sh` and `sh verify/formal/kitty/check.sh` ([ADR-026](docs/adr/ADR-026-formal-verification.md))

Every byte Tiles writes to a terminal comes from `crates/tiles-term`; a new output path joins the hostile-corpus tests over `fixtures/hostile/text.json`. A change to a chart form or the engine is also driven through the real `tiles` CLI with the `verify-tiles` skill in `.agents/skills/`.

## Decisions

Every decision has one record in [docs/adr/](docs/adr/README.md). A changed decision takes a new record with the next number, the old record's status links to it, and a correction to an old record links to the record that supersedes or amends it. Code and pull requests cite records by number.

## Golden images

The engine's tests compare every card with a golden baseline. A baseline is re-blessed (`TILES_BLESS=1 cargo test`) only for an intended visual change, and the pull request shows the before, after and diff images. Never re-bless to make a failing diff pass ([ADR-006](docs/adr/ADR-006-quality.md), [ADR-037](docs/adr/ADR-037-plain-theme-for-builds-from-source.md)).

## Commits and pull requests

Write every commit and pull request for a reader who has never seen this repository.

A commit's subject is a conventional commit, `type(scope): summary`:

- `type` is one of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci` or `chore`.
- `scope` is required: the crate, adapter or area the change touches, such as `tiles-engine`, `tiles-cli` or `adr`.
- The summary is imperative, lowercase, has no trailing period, and keeps the whole subject to 72 characters or fewer.
- One commit carries one outcome a user would notice.

The body says why the change was made and names the ADR that records the decision. It does not narrate how the work went, quote conversations or name local paths. When an agent wrote the change, the commit ends with a `Co-authored-by:` trailer naming the model.

A pull request follows [the template](.github/pull_request_template.md), including its "Built with" line, which names the model and harness, or "no AI" when none was used. Pull requests are squash-merged: the merge commit's subject is the pull request's title with `(#N)` appended, and its body is the pull request's Change section followed, when an agent wrote the change, by a `Co-authored-by:` trailer naming the model. Whoever merges adds that trailer; it does not depend on trailers in the branch's commits. Fix-up commits made on the branch, including the review pipeline's own, are left out of main and are not held to the subject convention.
