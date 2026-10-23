# ADR-026: Formal proofs check the painter's state machine and the kitty payload encoder beside the fuzz job

- Status: Accepted
- Date: 2026-10-19

## Context

The owner, 2026-10-19, on `create-formal-verification-skill` (beta), new in pstack, a Cursor skills plugin: "if very useful, lets use it both in our own development process so it helps us + incorporate it inside tiles too if its useful there too", and later the same evening, "If it finds valuable things, do fix them."

The skill prescribes no tool and ships no templates. It sets the bar a proof must meet: a property true to the code, shown to fail on a broken variant, a finding never weakened to pass, honest bounds, one rerun command, and a project-local skill the next agent can follow cold. A scout ranked five targets in Tiles: the kitty encoder's framing (Kani), the wrapper painter's synchronized-update and entrance state machine (TLA+), the Omni client's retry, deadline and redirect rules (TLA+), the intent parser's limits (Kani) and the cache's never-reuse-an-id rule.

## Decision

### The skill

`.agents/skills/create-formal-verification-skill/SKILL.md` is vendored unmodified from cursor/plugins at commit `4382e687a50b8d2c5ce4d141d001186589cff8d5` (blob `21fd16918fb3bb0e0ed109d1f43b92df2bb59bc9`), under the MIT text already in `LICENSE-pstack`, the same way as the two pstack skills before it (`.agents/skills/README.md`). It generated two project skills, `verify-painter-model` and `verify-kitty-encoder`; each proof area has a README under `verify/formal/<area>/` stating its properties, bounds, assumptions and what is left out.

### Targets and tools

| Target | Tool | Where | Bounds |
|---|---|---|---|
| The painter's kitty motion route: upload, entrance outbox, `Play`, early placement, hand-over, resize, `give_up`, against the harness's synchronized updates | TLA+ checked by TLC, `tla2tools.jar` 1.7.4 fetched by `scripts/fetch-tla2tools.sh` against a pinned SHA-256 | `verify/formal/painter/`, `sh verify/formal/painter/check.sh` | Two cards with two-row blocks for the four safety properties (5,508 states), one card with a three-row block for liveness; about 20 s |
| The kitty payload encoder, `tiles_term::b64::encode` | Kani 0.68.0 | `verify/formal/kitty/`, `sh verify/formal/kitty/check.sh` | Every input of 1, 2 and 3 bytes, one harness at a time within the limits in `verify/formal/kitty/README.md` |

Kani runs only on `tiles-term`, through `verify/formal/kitty`, a workspace of its own that depends on nothing else, so the main build never compiles it. It never runs on `tiles-engine`, `tiles-mcp` or `tiles-cli`, which link skia-safe. The painter lives in `tiles-cli`, so it is modelled in TLA+ rather than proven in Rust.

### A refactor for verifiability

`b64::encode` built its `String` with `String::push(char::from(..))`, and Kani could not tell each char was one byte: a 3-byte input never reached the solver in 238 s. It now builds a `Vec<u8>` and converts once with `String::from_utf8`, which cannot fail on ASCII. Behaviour and its unit tests are unchanged.

### Beside the fuzz job

The fuzz job (ADR-024) throws random and mutated input at every parser for 60 seconds and finds what it happens to reach. The proofs cover every input within their bounds and every interleaving the model allows, but only for the code they name. Neither replaces the other: the fuzz job stays unchanged, and the new `formal` CI job runs both check scripts on ubuntu-24.04 with Kani 0.68.0 pinned and `~/.kani` cached. The Kani bundle is pinned by SHA-256 like the other CI downloads (`tla2tools.jar`, gitleaks under ADR-024, the Skia archives under ADR-014): `scripts/fetch-kani-bundle.sh` checks it before `cargo kani setup --use-local-bundle` unpacks it. The TLC check is required now: a counterexample fails the job and blocks the PR, and the painter check is deterministic and takes about 20 s. The Kani install and the Kani proofs run as steps with `continue-on-error: true`, tolerated until two green runs show their runtime, then become required. The kitty check script is a check of point 20 of `docs/definition-of-done.md`, nothing raw reaches the terminal. The painter model guards placement and `Play`-state invariants that no point there names, so its script is in no Checked-by line; it joins the list, under a point naming what it guards, when the whole job is a gate.

### Findings

- **F1** (TLC, `GaveUpCleanly`, fixed): a painter that gave up after a panic deleted placement 2 of the card's settled image instead of the entrance it had placed early, so the entrance stayed on screen for good. `give_up` now calls `unplace_early`; test `a_broken_painter_removes_an_entrance_placed_early`.
- **F2** (found while modelling, fixed): after 65,793 ids `take_image_id` returned `0x510101`, the image id of eight cards, so deleting that entrance or backing could delete a card's picture. It now counts over even ids only, since every `card_image_id` is odd; test `no_painter_image_id_is_a_cards_image_id`.

## Consequences

- A change to the functions `verify-painter-model` names, or to `b64.rs`, reruns its check script; a change that breaks a property by design changes the property in the same commit and says why.
- The kitty framing is not proven yet. Harnesses for `kitty.rs`'s commands did not finish within 3 minutes on the 8 GB 2017 Mac; `verify/formal/kitty/README.md` lists each with its bound and measured time. Until one passes, the framing rests on the kitty unit tests, the hostile-corpus tests and the fuzz targets.
- Follow-ups: the Omni client's retry, deadline and redirect rules as a second TLA+ model (target 3), and the framing harnesses on a faster machine. The intent parser's limits (target 4) stay with the fuzz job and the property tests. The cache's id rule (target 5) is out of scope: the cache builds only with Skia, and F2's fix covers the id half of it.
- The skill is beta upstream. It is pinned, and an update is a clean re-copy after reading the diff; a change in its bar is a reason to revisit this record, not to rewrite the proofs silently.
