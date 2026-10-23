# ADR-042: The house categorical greys stand at least 3:1 on their card, and no theme lists a colour the contrast check lets pass

- Status: Accepted; supersedes [ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md)'s three house greys under 3:1 and [ADR-037](ADR-037-plain-theme-for-builds-from-source.md)'s `under_three_to_one` list in the tokens; dark `cat_2`, which it kept equal to `muted`, is lifted off it by [ADR-047](ADR-047-context-greys-and-no-red-category.md)
- Date: 2026-10-20

## Context

[ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md) ran the contrast check over every theme and found three house categorical colours under 3:1 on their card: light `cat_3` (2.55:1) and `cat_5` (1.65:1), and dark `cat_5` (2.19:1). [ADR-037](ADR-037-plain-theme-for-builds-from-source.md) moved that list into the house tokens as each theme's `under_three_to_one`, so the public test named no house colour. A series drawn in one of those greys could be hard to see without its label.

The owner compared the current greys with a lifted set side by side, drawn by `tiles_chart` in light and dark, and could not tell the two apart. Lifting only the three would break playbook K6's order: at 3:1 on white a grey sits at L\* 61.6 or darker, which leaves room for one more grey between it and light `cat_2` at L\* 44.7, not two that stay 10 apart for colour-blind readers; on the dark card `cat_5` at 3:1 lands only 7.5 L\* below `cat_3`. So two greys outside the three also move a little.

## Decision

- **The lifted greys.** Each new grey is its old colour shifted equally in R, G and B, so it keeps its blue-grey tint, and the greys keep K6's light-to-dark order.

  | Theme | Slot | Contrast before | Contrast now |
  |---|---|---|---|
  | light | `cat_2` | 5.43:1 | 6.50:1 |
  | light | `cat_3` | 2.55:1 | 4.41:1 |
  | light | `cat_5` | 1.65:1 | 3.02:1 |
  | dark | `cat_3` | 3.91:1 | 4.37:1 |
  | dark | `cat_5` | 2.19:1 | 3.03:1 |

  The colours are in the house tokens. Every other colour stays, the light `muted` text token included, so light `cat_2` no longer equals it. The closest pairs for colour-blind readers measure 10.2 (light `cat_2`/`cat_3` and `cat_3`/`cat_5`, protanopia) and 10.1 (dark `cat_3`/`cat_5`, deuteranopia), over the floor of 10.
- **No exemption.** Both `under_three_to_one` lists leave the house tokens, and the field leaves `Palette`, so tokens that name it no longer open. `tests/themes.rs` holds every categorical colour of every theme to 3:1 on its card.

A lighter variant of light and dark `cat_3` and `cat_5`, about 1.5 to 2 L\* lighter, was drawn too and parked for a later call: light `cat_5` would fall to 2.83:1, since its lifted grey is already the lightest of its tint that meets 3:1 on white.

## Consequences

- Pack 1's hash changes, so `scripts/design-pack.sha256` pins the new pack. Of the house goldens only `grouped-bar-light.png` changes, since it is the one fixture that colours a series by `cat_2`, `cat_3` or `cat_5`; it was re-blessed as an intended visual change under [ADR-006](ADR-006-quality.md). The plain goldens, the Obsidian goldens and every dark golden stay as they were.
- A future theme whose colour falls under 3:1 fails the test, with no list to add it to; making a colour exempt again takes a new record.
- Related: [ADR-006](ADR-006-quality.md), [ADR-021](ADR-021-obsidian-default-and-theme-switch.md), [ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md), [ADR-037](ADR-037-plain-theme-for-builds-from-source.md).
