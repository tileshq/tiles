# ADR-047: The lines beside a named line take ordered context greys, and no category is red

- Status: Accepted; changes playbook K1's and [ADR-010](ADR-010-engine-v0-1.md)'s one grey for every line beside a named line, the house categorical orders [ADR-021](ADR-021-obsidian-default-and-theme-switch.md) and [ADR-042](ADR-042-house-greys-meet-three-to-one.md) set, and the plain categorical order [ADR-037](ADR-037-plain-theme-for-builds-from-source.md) set
- Date: 2026-10-21

## Context

In the v0.1.9 laptop test the five-line chart's greys all read as one colour. The chart named one series, so playbook K1 drew the other four lines in the theme's single `grey` (light's stood 1.65:1 on its card). ADR-042's lifted categorical greys only appear on a chart that names nothing. The owner chose option 1a: on a chart that names a line, the other lines take their own ordered neutrals, each at least 3:1 on its card, so they can be told apart while the named line still leads. The owner expects more rounds on the exact shades.

The owner also asked why categorical slot 4 was red, since K2 makes red mean a bad finding, and decided to take it out: watch for near-reds too (a strong orange or magenta reads as bad, so check the slot's neighbours), replace it with a different hue such as teal, indigo or violet, keep green out the same way if the house rules tie green to good, and test the new set with a colour-blindness simulator. The house palette's standing choices for its purple, yellow and green hold.

## Decision

- **Context greys.** Each theme gains `context`, four greys in series order. On a multi-line or slope chart that names a series, the first unnamed line takes the first grey, the next the second, and every line past the fourth takes the last. Stacks and groups keep their `neutrals`; bars, dots and other single-series marks keep `grey`. Each list is one grey shifted equally in R, G and B, as ADR-042's greys were, so it keeps its tint. The loudest grey comes first.

  | Theme (card) | Context greys, first to last | Contrast on the card |
  |---|---|---|
  | Obsidian | four greys in the house tokens, Obsidian's own context grey lifted in four steps | 7.64, 5.59, 4.13, 3.03 |
  | light | four greys in the house tokens | 7.88, 5.53, 4.09, 3.02 |
  | dark | four greys in the house tokens | 7.72, 5.57, 4.10, 3.03 |
  | plain dark (`#22211e`) | `#b8b4ad`, `#9c9891`, `#848079`, `#6f6b64` | 3.04 for the last |
  | plain light (`#f9f7f2`) | `#524c42`, `#696359`, `#7e786e`, `#948e84` | 3.04 for the last |

  The house values live in the house tokens; this record gives the plain ones.

  The floor is 8 in CIEDE2000 between every two greys of a theme, with normal vision and with protanopia, deuteranopia and tritanopia simulated (Machado, Oliveira and Fernandes 2009, severity 1). The closest pairs measure 8.0 (Obsidian, deuteranopia and protanopia), 8.1 (light) and 8.2 (dark). An even L\* step does not give an even CIEDE2000 step, so each grey was moved until it stood 8.0 from the one before. The quietest grey of each house theme meets 3:1 at the first shift that does.
- **The named line still leads, by colour and weight, not by contrast.** Each context grey carries CIELAB chroma under 10 (5.0 Obsidian, 6.1 light, 8.9 dark at most) while the accent carries 61, 80 and 56; every context grey stands at least 16.1 from the accent in CIEDE2000 under every simulation (light, tritanopia). On a multi-line or a slope chart the named line keeps its 3 px stroke against 1.8 px (M4) and its name in its colour at weight 600 (M8); a slope's named line also ends in dots of radius 4.47 against 3 and writes both its values in its colour at weight 600. Slope used to draw every line at 2.2 px with dots of one size, so it takes multi-line's spotlight tokens here; with nothing named it draws as before. A context grey is not quieter than the accent by contrast: Obsidian's accent stands only 4.05:1, and four greys 8 apart from 3:1 cannot all sit below it, so the loudest grey (7.64:1) out-contrasts it. The same holds on light (accent 5.17:1) and dark (6.37:1).
- **No category is red, orange or magenta, in the house themes or the plain ones.** Red stays only for a bad finding (K2). The house rules give a good finding the accent (`Tone::Good`), not green, so green has no fixed meaning here, and no new category is green either.

  | Theme | Slot | Was | Now | Contrast | Nearest other category, CIEDE2000 (normal, deuteranopia, protanopia, tritanopia) |
  |---|---|---|---|---|---|
  | light | `cat_4` | red | a deep violet | 11.46:1 | 19.4, 17.9, 19.2, 19.7 |
  | dark | `cat_4` | red | the house violet | 3.67:1 | 14.8, 14.7, 14.8, 13.2 |
  | Obsidian | `cat_3` | orange | the house yellow, a colour of Obsidian's source unused before | 15.44:1 | 34.1, 33.0, 27.3, 20.5 |
  | Obsidian | `cat_4` | red | a pale lavender, Obsidian's focus text mixed 30% toward its ink | 12.22:1 | 19.7, 16.5, 12.7, 17.2 |
  | plain dark | `cat_3` | peach `#e09676` (hue 49) | slate violet `#6e6b94` | 3.22:1 | 22.6, 13.2, 15.2, 22.0 |
  | plain dark | `cat_4` | mauve `#9f6aa0` (hue 326) | indigo `#95aefb` (CIELAB chroma 42) | 7.46:1 | 15.5, 10.7, 10.6, 11.4 |
  | plain light | `cat_3` | terracotta `#ac7563` (hue 44) | slate violet `#8b87b1` | 3.16:1 | 23.6, 15.6, 15.7, 20.8 |
  | plain light | `cat_4` | plum `#684171` (hue 321) | indigo `#033874` (CIELAB chroma 40) | 10.77:1 | 15.6, 14.1, 15.8, 16.1 |
  | dark | `cat_2` | the theme's `muted` text colour | the same grey lifted by 16 in R, G and B | 9.08:1 | 17.9, 18.7, 18.5, 18.3 |

  Light and dark had greys on both sides of red, so only slot 4 moves for red. The house violet does not fit light: on white it stands 2.2 from the blue accent with tritanopia and 2.5 with protanopia, and darkening it to separate reads as navy beside the accent, so light takes a violet instead. On Obsidian, red's neighbour `cat_3` was a strong orange, so it went too. The sky blue stays in `cat_5`; the source's lavender as it stands measured only 6.9 from it with protanopia, so `cat_4` takes it mixed toward the ink until the two stand 10 apart. Obsidian now lists five colours instead of six; every theme already drew at most five series. `tools/themes/port_obsidian.py` builds the new order from the source file, so no Obsidian hex is typed by hand.

  The plain themes lose their orange and magenta the same way. Plain keeps the same hues in dark and light (ADR-037), so both take a slate violet in `cat_3` and an indigo of hue 284 in `cat_4`, light on the dark card and deep on the light one. A grey was tried in `cat_4` first and rejected: a tinted grey is not a different hue, and it stood within 3 of plain's own context greys, so series 4 read as a line beside a named one. None of the new colours is in the house file, whose violets are the house violet and a deep violet. Dark `cat_2` equalled the theme's `muted` text colour, so a series drew in the colour of the axis labels; it is lifted until it does not, and keeps playbook K6's light-to-dark order.
- **The checks.** `tests/themes.rs` holds every theme's context greys to 3:1 on the card, 8 apart under normal vision and the three simulations, and chroma under 10 with an accent of chroma 20 or more. It reads both the house and the plain tokens files, whichever the build seals, and skips the house file in a tree without it, for four more checks: no category has a hue angle under 78 degrees or over 312 unless it is a grey; each replaced slot stands 10 apart with tritanopia from the slots on either side of it, the other pairs being held only for deuteranopia and protanopia as before (Obsidian's teal and sky blue measure 8.5 with tritanopia); no series colour (the accent, `grey`, a category, a neutral or a context grey) equals a text or axis colour of its theme (`ink`, `muted`, `grid`, `line` or `track`); and every category stands at least 8 in CIEDE2000, with normal vision, from each context grey of its theme, so it never reads as a de-emphasised line.

  ADR-042's categorical greys fail that last check and stay as they are in this round; the test lists them by slot and fails if one of them comes to pass: light `cat_2` (5.1 from `context_2`), `cat_3` (2.5 from `context_3`) and `cat_5` (0.0, it is `context_4`), and dark `cat_2` (4.1 from `context_1`), `cat_3` (2.0 from `context_3`) and `cat_5` (0.0, it is `context_4`). On a light or dark chart that names no series, series 2, 3 and 5 can look like the lines beside a named one; moving those greys or the context greys apart is left to the next round on the shades.

## Consequences

- Pack 1's hash changes, so `scripts/design-pack.sha256` pins the new pack. The multi-line and slope goldens change in every theme, house and plain, since those fixtures name one of three or five series, and so does `grouped-bar-dark.png`, the one fixture coloured by dark `cat_2`; they were re-blessed as an intended visual change under [ADR-006](ADR-006-quality.md). No fixture draws three or more categories without a highlight, so no golden shows a new `cat_3` or `cat_4`.
- The prototype draws every unnamed line in one light grey, so the light multi-line and slope cards move further from its reference: `tests/reference.rs` loosens their bounds to about a tenth over today's distance (multi-line mean 6.998, visible share 0.0563; slope 6.016, 0.0467). The dark ones stay inside their bounds.
- A spotlight line chart with more than five lines still draws: lines past the fourth unnamed share the quietest grey, as all of them used to share one.
- Plain light still measures 7.3 between `cat_1` and `cat_2` with tritanopia, as does Obsidian's teal and sky blue at 8.5; neither pair holds a replaced slot, so neither is checked.
- Before and after cards for this decision, a five-line chart naming one line and a five-series grouped bar in light, dark and Obsidian, are kept outside the repository for the owner's review of the shades.
- Related: [ADR-006](ADR-006-quality.md), [ADR-021](ADR-021-obsidian-default-and-theme-switch.md), [ADR-036](ADR-036-design-pack-seam-and-deterministic-seal.md), [ADR-037](ADR-037-plain-theme-for-builds-from-source.md), [ADR-042](ADR-042-house-greys-meet-three-to-one.md).
