# ADR-055: A push to main skips CI's heavy jobs when its tree is one a pull request's CI passed on

- Status: Accepted; amends [ADR-007](ADR-007-stack-and-delivery.md)'s CI for pushes to main; [ADR-058](ADR-058-add-ons-and-fixture-cards-run-with-no-network.md)'s `no-network` job joins the gated jobs
- Date: 2026-10-22

## Context

`.github/workflows/ci.yml` runs on `pull_request` and on `push` to `main`. Every squash merge therefore runs the whole workflow twice: once on the pull request's head and again on the squash commit, macOS leg included. From 1 to 22 October, rounding each job up to the minute as GitHub bills it, the 62 pull request runs took 1,055 Linux and 99 macOS minutes and the 53 runs on main took 950 Linux and 102 macOS minutes, 55 of the Linux minutes in `policy`. In 24 of 25 merges the squash commit's tree was the tree of the pull request's head, so the second run tested files CI had already passed.

Nothing reads main's CI result. `release.yml` runs on a `v*` tag, its `preflight` job checks only that the releases token exists, and its `build` job runs `cargo test` itself; no script, branch rule or badge reads the run on main.

A pull request's CI checks out GitHub's merge ref, the head merged into main as main stood when the run started, not the head itself. On 20 October `policy` failed on main thirteen minutes after the same tree passed on its pull request: `cargo audit` found RUSTSEC-2026-0382, published in between.

## Decision

A `main-gate` job runs first on a push to main and skips on a pull request. It checks out only `scripts/ci-main-gate.py` and runs it with a token that can read contents, pull requests and Actions. The script writes `skip=true` only when the GitHub API shows all of these:

1. The pushed commit is the merge commit of exactly one merged pull request into `main` (`GET /repos/{owner}/{repo}/commits/{sha}/pulls`).
2. The pushed commit's tree is the tree of that pull request's head commit.
3. The head contains main as it was before the push (`github.event.before`), by the compare API's `ahead` or `identical`. The merge ref the pull request's CI tested then had the head's tree, so condition 2 means main now holds exactly the files that run tested.
4. The newest `pull_request` run of this workflow for that head, on the pull request's branch, completed with `success`, and each of its jobs but `main-gate` completed with `success`, none cancelled or skipped. A run that lists pull requests must list only this one into `main`; GitHub empties that list once the branch merges, so an empty list is accepted.

Anything else writes `skip=false`: a condition that fails, an HTTP error, a missing field, an unreachable API. The step has `continue-on-error` and a three-minute timeout, with twenty seconds for each API call, so a crash or a hung call leaves no output and the gate green; the jobs read an empty output as run everything. The log line, also written to the job summary, names the pull request, its head SHA, the tree and the CI run relied on, or the condition that failed.

`rust` (both legs), `release-binary`, `house`, `adapters`, `fuzz`, `formal` and `secrets` need `main-gate` and run unless it says `skip=true`. `policy` stays outside the gate and runs on every push to main, because its advisory checks can fail on an unchanged tree. Pull requests run every job as before, the macOS leg on every push, and the job names their checks use are unchanged.

## Consequences

- A skipped main run ends green with `main-gate` and `policy` passed and the heavy jobs shown as skipped. A run the gate cannot prove runs every job, as before.
- A flaky failure that passed once on the pull request is no longer sampled a second time on main.
- The `secrets` job is skipped with the rest: gitleaks reads file contents, and every line the squash commit adds is in the head's tree and its commits, which the pull request's run scanned.
- `release.yml` is unchanged and still reads no CI result. Its `preflight` has no green-CI gate, and `cancel-in-progress` can leave a tagged commit's main run cancelled; both gaps predate this record.
- Each push to main pays for one short Linux job before the others start; a pull request shows `main-gate` as a skipped check.
- A rebase merge of several commits fails condition 3, since its pushed commit's parent is not in the head, and runs everything.
- Tests: `CiMainGate` in `scripts/test_policy_scripts.py`, over the recorded API answers in `fixtures/github/ci-main-gate/` (a skip, a tree mismatch, a cancelled run, a 403) and edits of them.
