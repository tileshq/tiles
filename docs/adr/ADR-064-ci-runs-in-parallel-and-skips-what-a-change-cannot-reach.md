# ADR-064: CI runs its gates as parallel jobs, skips what a change cannot reach, and gates outside pull requests on a label

- Status: Accepted; amends [ADR-007](ADR-007-stack-and-delivery.md)'s CI (the `rust` gate runs as two jobs per system, and a push to a branch runs light checks), [ADR-024](ADR-024-security-baseline-in-ci.md)'s `fuzz` job (one job per target) and [ADR-055](ADR-055-main-skips-a-tree-its-pull-request-passed.md)'s gated jobs (they also skip on the plan below)
- Date: 2026-11-26

## Context

The nine pull request runs of `.github/workflows/ci.yml` from 23 October to 1 November 2026 took 347 to 550 seconds from start to finish, median 395. Each job's median and maximum, in seconds:

| Job | Median | Max |
|---|---:|---:|
| `fuzz` | 391 | 394 |
| `rust (macos-15)` | 148 | 171 |
| `formal` | 134 | 143 |
| `rust (ubuntu-24.04)` | 114 | 129 |
| `release-binary` | 93 | 104 |
| `no-network` | 25 | 30 |
| `policy` | 21 | 25 |
| `adapters` | 11 | 21 |
| `secrets` | 7 | 9 |

`fuzz` alone set every run's length. Its one step ran the five targets one after another for 61 seconds each (305 s), after 25 s of building and 21 s compiling cargo-fuzz. In the `rust` job on macOS, clippy took 39 s, building the tests 37 s and running them 43 s, of which `tiles-cli`'s `tests/pty.rs` took 18 s as one binary.

No job restored a cache. Every `Swatinem/rust-cache` step logged "No cache found", and the repository listed no cache. A pull request can restore only caches saved on its own merge ref or on `main`, and since ADR-055 a push to main skips the jobs that build, so main saved none and every pull request built Skia's bindings, every crate and cargo-fuzz from nothing.

The ruleset on `main` requires nine checks by name: `rust (ubuntu-24.04)`, `rust (macos-15)`, `release-binary`, `adapters`, `no-network`, `policy`, `fuzz`, `formal` and `secrets`. A job skipped by its condition reports success to a required check, but a matrix job skipped that way publishes its unexpanded name, such as `rust (${{ matrix.os }})`, so a required check under a system's name never reports and the pull request cannot merge.

Firstmate's CI, reviewed on 26 November, shards its suites to a measured time budget, warns when a job passes it, and runs outside pull requests' full suite only after a maintainer adds a `full-ci` label.

## Decision

### Parallel jobs, under the same required names

- The `rust` gate runs as `rust lint (<system>)` (rustfmt and clippy) and `rust test (<system>)` (`cargo test`, and on macOS the installer and package tests) on Ubuntu and macOS at once. Each system runs the same steps it ran before. `rust (<system>)` is a final job that runs whatever its jobs did and fails when either failed or was cancelled. It reads the matrix's combined result, so a failure on one system fails both systems' checks; the `rust lint` and `rust test` checks name the system that failed.
- The fuzz gate runs `fuzz (<target>)` for each target at once, six since `pane_input` joined,, each still 60 seconds under AddressSanitizer from its seed corpus, keeping a crashing input as `fuzz-crashes-<target>`. `fuzz` is the final job. `CiWorkflow` in `scripts/test_policy_scripts.py` fails when the matrix's list differs from `fuzz/fuzz_targets/`.
- The tests were not split further. A `cargo nextest` partition, or a split by crate, would build every test binary again in each part, 31 to 37 s, to save part of a 34 to 43 s run whose longest binary is 13 to 18 s.

### A cache pull requests can restore

Once a day, and on demand, every job runs on `main` (`schedule` and `workflow_dispatch`) and saves its cache. Every `rust-cache` step saves only on `main`, so pull requests restore that run's caches and do not spend the repository's 10 GB on their own. `rust lint` and `rust test` use the shared keys `clippy` and `test`, and the fuzz targets share `fuzz`, which also keeps cargo-fuzz in `~/.cargo/bin`. The daily run on main also catches a failure that only appears on main, such as a new RustSec advisory or a fuzz crash, within a day.

### Skipping what a change cannot reach

A `plan` job runs `scripts/ci-plan.py` over the files a pull request's merge commit changes against main, or a push to main changes against the commit it replaced:

- Every changed file is docs text: `docs/` except `docs/install.md`, which `scripts/test_install.sh` reads, and `README.md`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `SECURITY.md` or `TRADEMARKS.md`. The `rust` jobs, `release-binary`, `no-network`, `fuzz` and `formal` skip. `policy`, which checks these files, `adapters` and `secrets` run.
- Every changed file is docs text or under `adapters/`. `fuzz` and `formal` skip. `tiles-cli` embeds the add-ons, so the `rust` jobs, `release-binary` and `no-network` run.
- Anything else, a diff git cannot produce, an empty diff, a scheduled or manual run, runs every job. A job skips only on the output `true`, so a crash of the plan, which leaves no output, runs every job.

`.agents/`, `LICENSE`, `fuzz/README.md` and every other path are not in the lists: a Rust test reads `.agents/skills/verify-tiles/features/`, and a path not shown to be unread runs the jobs.

### Outside pull requests and the `full-ci` label

A pull request is outside when it comes from a fork and its author's association is not `OWNER`, `MEMBER` or `COLLABORATOR`. Without the `full-ci` label it runs `policy` and `secrets` only. A new `full suite` check fails until a maintainer adds the label, and on every run it fails when any job failed or was cancelled; it reads the pull request from the event, not from `plan`, so a broken plan cannot open it. Since a skipped job satisfies a required check, the ruleset must require `full suite` beside the nine. Adding the label starts the full suite, and later pushes run it again. Adding any other label runs every job again, as a push does: a run that skipped would publish skipped checks over the ones it replaced and hide a failure.

### Light checks on a branch push and a time budget

- `.github/workflows/branch.yml` runs rustfmt, clippy and the add-ons' node tests on Linux on every push to a branch other than main, unless the push changes only docs text. None of its checks is required. It restores main's `clippy` cache and saves none. `claude plugin test` stays in `ci.yml`, whose `adapters` job pins the one Claude Code version `scripts/check-docs.py` checks.
- A `time budget` job runs after the others, writes every job's run time to the run's summary and prints a warning for each job over 180 seconds. It never fails. 180 seconds is above every job measured before this record but `fuzz` (the slowest, `rust (macos-15)`, peaked at 171), so a warning marks a job that has grown and should be split or sped up.

## Consequences

- A pull request's longest chain was `fuzz` at about 390 s. It is now the longest of a fuzz target (about 140 s cold), `formal` (134 s, set by Kani's 111 s), `rust test (macos-15)` and `release-binary` followed by `no-network`, plus the `plan` job's few seconds first and the final jobs' few seconds after.
- A pull request runs 22 jobs instead of 10, and the daily run on main adds one full run a day. The repository is public, so GitHub's standard runners, macOS included, do not bill these minutes; on a private repository the macOS share would cost ten times as much again, as ADR-007 notes.
- A pull request that changes only docs text, or only docs and add-ons, shows the skipped jobs as skipped. ADR-055's gate then does not skip main's push, since it needs every job of the pull request's run to have passed, but `plan` skips the same jobs on main.
- Moving `fuzz`, `formal` or the macOS leg off the pull request, self-hosted runners and batching pull requests would each change what guards a merge or need hardware, and are left to later records.
- The `full-ci` label must exist in the repository before a maintainer can add it.
- Tests: `CiPlan`, `CiBudget` and `CiWorkflow` in `scripts/test_policy_scripts.py`; `CiWorkflow` fails when a required check has no job, when a matrix job behind a required check can skip, when the fuzz matrix misses a target, and when `full suite` or `time budget` does not wait for every job.
