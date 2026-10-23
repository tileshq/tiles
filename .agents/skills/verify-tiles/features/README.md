# Tiles verification map

This directory is the maintained source for verifying what a Tiles user can do. Read this index, then use the matching feature file as the recipe.

## Baseline preconditions

- Build the release binary with design pack 1, as the skill's Launch section shows, for every feature file but the plain theme; build it without the pack for [the plain theme](./plain-theme.md).
- `.agents/skills/verify-tiles/scripts/doctor.sh` passes.
- Choose a run id no earlier run used; evidence goes to `target/verify/<run>/`.
- Drive only sessions this run started (`tiles-verify-<run>-<name>`).

## Driving conventions

- Run every command from the repository root through `scripts/drive.sh`.
- Use the fixture intents in `crates/tiles-engine/tests/intents/` unless a recipe says otherwise.
- Keep commands literal; `$OUT` stays single-quoted.

## Proof and skip reporting

- Proof is the transcript, the exit code and the PNG you opened and checked against the recipe's end state.
- Record the feature file and sub-feature id with each run directory.
- Report a recipe you could not run with the command and the unmet precondition; never mark it verified through another path.

## Feature entry contract

Each feature file starts with an H1 and one paragraph on the user-visible behaviour, then four H2s in order: `Sub-features`, `How to get to it (user POV)`, `Driving it with drive.sh` (starting with `Preconditions:`), `Gotchas`.

## Features

Surfaces:

- [Render and its errors](./cli-render.md): file and stdin input, options, refusals, hostile text.
- [List the forms](./cli-forms.md): the form list and roles.
- [The plain theme](./plain-theme.md): a build from source carries plain dark and light, dark the default, and draws every form in both.
- [Themes and /theme](./cli-theme.md): the default, `tiles theme`, the `tiles_theme` tool and prompt, earlier cards switching in kitty, `tiles open`.
- [Open in a window](./cli-open-window.md): `tiles open <id>` showing a card or dashboard in a macOS window, and the result line naming it.
- [The MCP server](./mcp-server.md): initialize, the tool list, text-only results with the block or without, refusals.
- [tiles run, per harness](./run-harnesses.md): Claude Code, Codex, OpenCode and Pi launched with Tiles loaded, and the line naming a harness version Tiles was not proved on.
- [Terminal routes](./terminal-routes.md): kitty placeholders, pre-cut, sixel, text, and detection.
- [Install and doctor](./install-doctor.md): permanent installs, the report, `tiles check`, `tiles cache clear`, the installer, `tiles update`, the update check, `tiles --licence` and a build that takes a design pack.

Chart forms, one per file:

- [column](./column.md): "Signups dipped in W3".
- [bar](./bar.md): "Northwind leads new revenue by a wide margin".
- [line](./line.md): "Weekly active teams climbed all year".
- [area](./area.md): "Storage use doubled after the June launch".
- [big-number](./big-number.md): "Annual recurring revenue reached $4.82M".
- [multi-line](./multi-line.md): "Self-serve win rate rose while sales-led held flat".
- [grouped-bar](./grouped-bar.md): "Every region but South closed more deals in 2026".
- [stacked-bar](./stacked-bar.md): "Enterprise accounts nearly doubled over the year".
- [stacked-100-bar](./stacked-100-bar.md): "Chat carries the most support in the Nordics and Benelux".
- [lollipop](./lollipop.md): "Billing sync fails most often on retries".
- [dot-plot](./dot-plot.md): "Support replies within 4 hours in every region but APAC".
- [slope](./slope.md): "Search overtook paid social as the top source".
- [dumbbell](./dumbbell.md): "Routing got faster in every queue, most in Billing".
- [bullet](./bullet.md): "Three of four teams are on track for quota".
- [waterfall](./waterfall.md): "Expansion more than covered churn this quarter".
- [scatter](./scatter.md): "Larger accounts adopt more features, Globex is the outlier".

