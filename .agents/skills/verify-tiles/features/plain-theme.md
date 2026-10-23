# The plain theme

A build without a design pack, which is what the published source builds, seals the plain tokens in `crates/tiles-engine/assets/plain/tokens.json` and carries two themes: `dark`, a charcoal card with a cyan accent and the default, and `light`, a warm white card with a teal accent (ADR-037). Every form draws in both with the same marks and roles as the house build: the accent on what the title names, grey on the rest, red for a bad finding. Only the colours, the card's corners and shadow, and the marks' finish differ.

## Sub-features

- `plain-licence` shows the binary was built from source.
- `plain-theme-list` lists dark, marked as the default, then light, each with its few words.
- `plain-refused` refuses `obsidian`, which only the house build carries, naming dark and light.
- `plain-<form>-dark` and `plain-<form>-light` draw each form's fixture in each plain theme.

## How to get to it (user POV)

- Build `tiles` from the source with no `TILES_DESIGN_PACK`: `cargo build --release -p tiles-cli`.
- Run `tiles theme` to see the themes, and `tiles render <intent.json> -o <out.png>` to draw a card in dark, or with `--theme light`.

## Driving it with drive.sh

Preconditions:

- The release binary was built without `TILES_DESIGN_PACK`, and `scripts/doctor.sh` passes and prints `doctor: ok carries Design assets (built from source)`.
- Unset `TILES_THEME`, and pass `--env TILES_CONFIG_DIR=<an empty directory under target/verify/<run>/>` to each drive below, written as an absolute path (`--env` values are not given `$OUT`), so no saved theme applies.

- **Licence.** `drive.sh <run> plain-licence --licence`: the second line starts `Design assets (built from source)`.
- **Theme list.** `drive.sh <run> plain-theme-list --env TILES_CONFIG_DIR=<dir> theme`: exit 0, the lines `* dark      charcoal card, cyan accent (the default)` and `  light     warm white card, teal accent`, then `Cards are drawn in dark (nothing chosen yet, so the default).`
- **Obsidian refused.** `drive.sh <run> plain-refused --env TILES_CONFIG_DIR=<dir> theme obsidian`: exit 1 and `tiles: "obsidian" is not a theme; use dark, light`.
- **Every form.** For each form in `tiles forms`, `drive.sh <run> plain-<form>-dark --env TILES_CONFIG_DIR=<dir> render crates/tiles-engine/tests/intents/<form>.json -o '$OUT/card.png'` and the same with name `plain-<form>-light` and `--theme light`. Each exits 0 and names `<form> form, dark theme` or `light theme`. Open each PNG: the card is charcoal with light text in dark and warm white with dark text in light, the marks the form's own feature file names carry the accent (cyan in dark, teal in light) or red, the others are warm grey, no text overlaps another, and nothing runs off the card.

## Gotchas

- The form feature files name the house colours (Obsidian's purple, the blue light accent); on this build read them for which mark carries the accent and which values are written, and expect the plain colours.
- `tests/golden-plain/` holds this build's goldens at scale 1; the CLI draws at scale 2.
- Switching between a build with the pack and one without rebuilds the engine, since the pack changes what the build script seals.
