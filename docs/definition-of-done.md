# Definition of done

Every pull request meets every point below before it merges, whether a person or an agent wrote it. The PR template's sections answer the points a PR must show; a point that does not apply needs no line.

Each point ends with a **Checked by:** line naming what enforces it: a CI job (`ci`), a lint configured in `Cargo.toml` (`lint`), a section of `deny.toml` (`deny`), a test file (`test`), a script (`script`), a file (`file`), or `maintainer review` when no machine checks it yet; the review pipeline checks those on every PR and raises a question for the maintainer when it is unsure. `scripts/check-docs.py` fails CI if a point has no such line or names a job, lint, section or file that does not exist. A rule nothing checks does not hold, so a new point arrives with its check, and a point checked only by review says which round adds its machine check.

The reasons are in [ADR-007](adr/ADR-007-stack-and-delivery.md) for engineering, [ADR-006](adr/ADR-006-quality.md) for tests and verification and [ADR-009](adr/ADR-009-security-baseline.md) for security; the craft rules the engine must match are in [card-rules.md](card-rules.md).

## Engineering

1. **The PR explains itself.** Its description says what was wrong, what changed, what was run and what could not be checked, and ends with the model and harness that did the work.
   Checked by: file `.github/pull_request_template.md` · maintainer review
2. **CI is green.** Every gate passes on Linux and macOS, with no skipped, deleted or weakened tests. Every review finding is fixed or answered before merge.
   Checked by: ci `rust` · ci `policy`
3. **The code is formatted** by rustfmt with the repository's settings.
   Checked by: ci `rust` · file `rustfmt.toml`
4. **Clippy passes with warnings as errors**, with the `all` and `pedantic` groups on for every crate. A lint is allowed only at the narrowest item, with a comment saying why.
   Checked by: ci `rust` · lint `clippy::all` · lint `clippy::pedantic`
5. **No unsafe code and no undocumented public item** in our crates. skia-safe holds the unsafe boundary (ADR-002).
   Checked by: lint `unsafe_code` · lint `missing_docs` · ci `rust`
6. **Every dependency has a permissive licence.** The allow-list in `deny.toml` refuses GPL, AGPL and LGPL, and any licence it does not name. Copyleft is refused because the official binary carries a proprietary design pack and the paid build links closed crates ([ADR-038](adr/ADR-038-open-core-split.md)), and a copyleft dependency's terms would reach both.
   Checked by: ci `policy` · deny `licenses` · file `deny.toml`
7. **No dependency has a known advisory, and every dependency comes from crates.io.** No git dependencies, no other registry. cargo-deny and cargo-audit both read the RustSec database (ADR-024).
   Checked by: ci `policy` · deny `advisories` · deny `sources` · deny `bans`
8. **A new dependency** carries a one-line reason and its licence in the PR, and `Cargo.lock` is committed with it. A version must be public for at least three days before we take it.
   Checked by: file `Cargo.lock` · file `.github/pull_request_template.md` · maintainer review
9. **Only the data source crates can reach the network.** `tiles-net` and `tiles-omni` are the owners; no other crate reaches a networking crate at run time except through them, or names a socket type (ADR-005, ADR-031).
   Checked by: script `scripts/check-network-owners.py` · test `scripts/test_policy_scripts.py` · file `docs/adr/ADR-031-warehouse-sources.md` · ci `policy`
10. **Architectural changes** add or supersede a record in `docs/adr/`.
    Checked by: file `docs/adr/README.md` · maintainer review
11. **An agent mistake that can repeat is fixed in structure first**: a type, a lint, a test or a CI check. Prose in `AGENTS.md` is the last resort, for what structure cannot hold.
    Checked by: file `AGENTS.md` · maintainer review
12. **GitHub Actions are pinned to commit SHAs**, with the version in a comment.
    Checked by: file `.github/workflows/ci.yml` · maintainer review

## Tests

13. **Every rendered form has golden images, asserted by image diff and never by eye.** A failing diff is fixed in the engine, never by editing the baseline. Baselines are regenerated (`TILES_BLESS=1`) only for an intended visual change, and the PR shows the before, after and diff images and why the pixels changed.
    Checked by: test `crates/tiles-engine/tests/golden.rs` · file `.github/pull_request_template.md` · maintainer review
14. **Each machine-checkable craft rule in the card rules has a scene-level test** that fails with the rule's id, and the rule's row names it. A new rule arrives with its test.
    Checked by: test `crates/tiles-engine/tests/rules.rs` · file `docs/card-rules.md` · maintainer review
15. **Pure logic gets a unit test.** No test asserts on source text, copies a list it should derive, or only checks that a mock returned what it was told to return.
    Checked by: maintainer review
16. **A bug-fix PR says its new test fails on the old code**, and how that was checked.
    Checked by: file `.github/pull_request_template.md` · maintainer review
17. **Untrusted input never panics.** Every parser of an intent, rows or an outside response has tests with malformed, oversized and hostile input and a property test over random input, and every form draws or refuses odd data without panicking. The terminal stream, the terminal's replies, intents and placeholder blocks each have a fuzz target CI runs for 60 seconds; a new parser adds one (ADR-024).
    Checked by: test `crates/tiles-intent/src/tests.rs` · test `crates/tiles-intent/tests/property.rs` · test `crates/tiles-engine/tests/property.rs` · test `crates/tiles-term/tests/property.rs` · test `crates/tiles-engine/tests/rules.rs` · file `fuzz/Cargo.toml` · ci `fuzz` · file `docs/adr/ADR-009-security-baseline.md` · maintainer review

## Verification

18. **The real thing is driven, not assumed.** Once the CLI renders, `verify-tiles` exists under `.agents/skills/`, made with `create-verification-skill`, with Launch, Doctor, Drive, Evidence and Cleanup and a feature map per chart form and per surface. Every later round runs `maintain-verification-skill` on the features it touched and states the outcome in the PR.
    Checked by: file `.agents/skills/verify-tiles/SKILL.md` · file `.agents/skills/create-verification-skill/SKILL.md` · file `.agents/skills/maintain-verification-skill/SKILL.md` · file `.github/pull_request_template.md` · maintainer review

## Security

19. **Every change to a surface has a security review** against the ADR-009 threat model in the PR: the MCP tools, a CLI command, the wrapper, a harness add-on, a data source.
    Checked by: file `.github/pull_request_template.md` · file `docs/adr/ADR-009-security-baseline.md` · maintainer review
20. **Nothing raw reaches the terminal.** All text Tiles prints goes through the one sanitizer and all graphics through the one encoder (`crates/tiles-term`), and the hostile-text corpus in `fixtures/hostile/text.json` runs through every output path: the placeholder block, the text summary, MCP results, the wrapper's paint, `tiles doctor`, `tiles check` and `tiles cache clear`. A new output path adds itself to those tests.
    Checked by: test `crates/tiles-term/tests/hostile.rs` · test `crates/tiles-mcp/src/server/tests.rs` · test `crates/tiles-cli/src/wrap/paint/tests.rs` · test `crates/tiles-cli/src/doctor.rs` · test `crates/tiles-cli/src/check.rs` · test `crates/tiles-cli/tests/cache.rs` · script `verify/formal/kitty/check.sh` · file `docs/adr/ADR-009-security-baseline.md` · file `docs/adr/ADR-026-formal-verification.md` · maintainer review
21. **Credentials appear in no output and no commit.** They come only from the environment or the Keychain, and a planted fake key is found in no log, error or tool result. The planted-key tests cover every Omni client call and every `tiles omni` command; each new output path adds itself to them. gitleaks finds no secret in the tree or any commit; an allowance in `.gitleaks.toml` names one file, rule and value (ADR-024).
    Checked by: test `crates/tiles-omni/tests/replay.rs` · test `crates/tiles-cli/src/omni.rs` · file `docs/adr/ADR-008-data-sources.md` · ci `secrets` · file `.gitleaks.toml` · maintainer review
22. **Nothing leaves the machine** except the user's own queries to the data source crates they configure: no telemetry, crash report, analytics or hosted call, and no personal data collected. The one exception is the daily update check, an anonymous `GET` to `api.github.com` that `TILES_UPDATE_CHECK=off` stops, and the binary never updates itself (ADR-032). No crate reaches a telemetry crate, shipped code names no host but Omni's, BigQuery's two (ADR-031) and, in the update code only, GitHub's two (ADR-032), and the add-ons reach no network and run only `tiles` (ADR-024). Rows fetched from Omni never reach the disk (ADR-027); `tiles run` denies Claude Code's and OpenCode's file and shell tools the card cache, and Pi's extension asks before a chart that fetches from Omni (ADR-028) or runs a warehouse query, and before each warehouse lookup (ADR-034).
    Checked by: script `scripts/check-network-owners.py` · test `scripts/test_policy_scripts.py` · test `crates/tiles-cli/tests/open.rs` · test `crates/tiles-cli/src/harness.rs` · test `adapters/pi/index.test.ts` · test `crates/tiles-cli/src/update/tests.rs` · test `crates/tiles-net/src/lib.rs` · file `docs/adr/ADR-005-privacy-and-secrecy.md` · file `docs/adr/ADR-024-security-baseline-in-ci.md` · file `docs/adr/ADR-032-update-command-and-harness-versions.md` · file `SECURITY.md` · script `scripts/no-network.py` · script `scripts/drive-addon-paths.py` · file `docs/adr/ADR-058-add-ons-and-fixture-cards-run-with-no-network.md` · ci `policy` · ci `no-network`
23. **A release binary carries no readable form, token or colour and no source path.** The forms and tokens ship sealed, and a release build remaps every source path (ADR-022).
    Checked by: script `scripts/check-release-binary.py` · script `scripts/release-rustflags.sh` · ci `release-binary`

## Keeping this file honest

24. **Every point and every playbook rule names a check that exists, and every doc link resolves.** The policy scripts have tests that show each one fails on the case it exists to catch.
    Checked by: script `scripts/check-docs.py` · test `scripts/test_policy_scripts.py` · ci `policy`
25. **No plans, research notes or agent scratch files are committed**; the merged PR is the record.
    Checked by: file `.gitignore` · maintainer review
