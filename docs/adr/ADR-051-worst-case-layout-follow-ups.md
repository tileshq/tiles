# ADR-051: A title wraps at the card's width, category labels thin pair by pair, a lollipop value clears its average, and a share short of an end says so

- Status: Accepted
- Date: 2026-10-21

## Context

The worst-case goldens ([ADR-049](ADR-049-worst-case-cards.md)) showed five layout faults beyond the fonts [ADR-050](ADR-050-macos-system-fonts-behind-inter.md) settled:

- Both token files set `title_wrap_chars` to 44, and playbook T2 said a title is "wrapped near 44 characters with balanced lines", but the engine never read the token: a title wrapped at the card's width.
- `bottom_labels` drew every n-th category label, with n set by the widest label against the band spacing. One 30-letter name (`Benachrichtigungseinstellungen`) set n to 3, so two of six columns were named on column, line, area, area-labelled, waterfall, grouped-bar and stacked-bar.
- A lollipop wrote each value 4 px past its head whatever lay there, so on `lollipop-worst` the label `12.0%` crossed the dashed rule of `Average 36.1%`.
- A stacked-100-bar wrote each share rounded to a whole percent. 1,284,000.5 beside 42.5 read `100`, and the other segment, under a pixel wide, had nothing to say it was there; its tooltip read `share: 0%`.
- A one-row lollipop with no `average` field drew an average rule through its own head, named with its own value.

## Decision

- **A title wraps at the card's width.** The title keeps the balanced wrap at the card's inner width, and `title_wrap_chars` is removed from `assets/tokens.json`, `assets/plain/tokens.json` and `TypeTokens`; playbook T2 now says what the engine does. A 44-character limit was tried and measured first: it pushed 9 of the 15 prototype references in `tests/reference.rs` past their bounds (column-highlight, multi-line, slope, dumbbell, bullet, lollipop, stacked-100-bar, scatter and dot-plot; lollipop and stacked-100-bar by 19 px of height where 0 is allowed). The approved gallery cards set titles of 52 to 73 characters on one line, and only the prototype's `tiles_core.build_spec` cut a title at 44 characters, so the gallery is the bar and the reference bounds stay as they are.
- **Category labels thin pair by pair.** Each label under the plot, moved in from the card's edge first as ADR-049 set, is drawn when its left edge clears the right edge of the label drawn before it by 8 px, and is left out otherwise. A long name hides only the neighbours it would touch: on `column-worst`, four of six columns are named where two were. Labels of even width thin to the same every-other pattern as before. The tick labels on a lollipop's, dumbbell's and bullet's value axis and a scatter's x axis keep the fixed step: every n-th tick is drawn, with n set by the widest tick against the tick spacing, so the scale reads at an even step (0, 40, 80, 120, never 0, 20, 60, 100); the card-edge move and the 8 px check apply there too.
- **A lollipop value clears its average.** A value label whose box would come within 4 px of the dashed rule is written 4 px past the rule instead. The value scale leaves room for that: it narrows until the widest value label fits 4 px past the rule inside the card's padding, which only bites when an average the intent sends sits near or above the rows. The rule's name sits above the rows, so a value label never meets it.
- **A share that only rounds to an end says so.** A share above 0% that rounds to 0 reads `<1`, and one below 100% that rounds to 100 reads `>99`, in the segment and in its tooltip. A share of exactly 0 or 100 reads as before.
- **One row has no mean of its own.** A lollipop with fewer than two rows draws no rule at the rows' mean and no name, and drops the line of room above the rows that the name took. An average the intent sends in `fields.average` is not the row's own value, so it still draws its rule and name.

## Consequences

- No golden changes for the title: the token was never read. A title wider than the card, such as the 90-character `-worst` titles, still takes two balanced lines. Pack 1 seals the house tokens, so its hash changes and `scripts/design-pack.sha256` pins the new pack.
- Only worst-case goldens change: 19 in each golden set, `worst-case/` and `worst-case-macos/` together. No fixture or prototype reference holds a label set of uneven widths that thins, a value on the average, a share that rounds to an end, or a one-row lollipop, and `tests/reference.rs` stays inside its bounds.
- Category labels of uneven width can now thin unevenly, for example the first, third and fourth of six, which still names more categories than a fixed step did. A value axis never does.
- A value moved past the rule reads a little farther from its head than the others.
- `tests/rules.rs` checks each rule: a long name leaves at least five of six short names on a column and a line chart, a lollipop, dumbbell, bullet and scatter value axis reads at an even step at every card width from 280 to 400 px, no lollipop value crosses its rule or touches its name, even beside an average the intent sends above its rows, a share short of 100 reads `>99` and its partner's tooltip `<1%`, and a one-row lollipop draws no rule at its own mean but does draw an average the intent sends.
