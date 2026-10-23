# Agent skills

| Skill | Source | Use |
|---|---|---|
| `create-verification-skill/` | pstack, vendored unmodified | Generates `verify-tiles/` (ADR-006) |
| `maintain-verification-skill/` | pstack, vendored unmodified | Every round after the engine round runs it on the features it touched |
| `create-formal-verification-skill/` | pstack, vendored unmodified, at commit `4382e687a50b8d2c5ce4d141d001186589cff8d5` (blob `21fd16918fb3bb0e0ed109d1f43b92df2bb59bc9`, fetched 2026-10-19, beta) | Generates the formal verification skills, `verify-kitty-encoder/` and `verify-painter-model/`, and their proofs under `verify/formal/` |
| `verify-tiles/` | Generated with `create-verification-skill` in the engine round | Drives the `tiles` CLI per chart form and keeps the evidence (ADR-006) |
| `verify-painter-model/` | Generated with `create-formal-verification-skill` | Model-checks the wrapper painter's kitty motion route in TLA+ with `sh verify/formal/painter/check.sh` (ADR-026) |
| `verify-kitty-encoder/` | Generated with `create-formal-verification-skill` | Proves the kitty payload encoder, `tiles_term::b64::encode`, with Kani through `sh verify/formal/kitty/check.sh` (ADR-026) |
| `review-animations/` | emil-skills by Emil Kowalski, vendored unmodified, at commit `2d228630ba1446365f614a108cb8252f1feda202` (fetched 2026-10-19) | Strict review of every entrance or motion change (card rules N1, N2) before a round is called done |

The first three come from pstack by Lauren Tan, https://github.com/cursor/plugins/tree/main/pstack, under the MIT licence in `LICENSE-pstack`: the first two at commit `0317eb8b858c991c57721358daaee19dbe8c814d` (fetched 2026-10-17), the third at the commit in its row, whose licence text is unchanged. They are vendored unmodified so an update is a clean re-copy. Two things differ in Tiles: the generated skills (`verify-tiles/`, `verify-painter-model/`, `verify-kitty-encoder/`) live under `.agents/skills/` rather than `.cursor/skills/`, and no Cursor plugin is needed to run any of them.

`review-animations/` comes from emil-skills by Emil Kowalski, https://github.com/emilkowalski/skills, under the MIT licence in `LICENSE-emil-skills`, vendored unmodified (`SKILL.md` and `STANDARDS.md`) so an update is a clean re-copy. Its rules speak CSS; in Tiles the animated properties are the fields of `Moment` in `crates/tiles-engine/src/motion.rs` (`opacity`, `lift`, `scale`, `grow`, `fade`), the curves and times are the `motion` tokens, and frames show at most one per 50 ms tick (ADR-030). Its frontmatter sets `disable-model-invocation: true`, so a round's brief names it.
