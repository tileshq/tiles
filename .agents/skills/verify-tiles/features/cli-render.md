# Render and its errors

`tiles render` reads one intent (a file, or `-` for stdin), writes one PNG atomically, and prints one line naming the file, form, theme and pixel size. Every failure prints one line to stderr that names the key, the problem and the fix, exits non-zero and writes no file.

## Sub-features

- `render-file` draws an intent file to a PNG at scale 2.
- `render-stdin` reads the intent from stdin with `-`.
- `render-options` honours `--theme`, `--width`, `--scale`, `--frames` and `--timings`; with no `--theme` the theme from `tiles theme` or `TILES_THEME` applies, else Obsidian.
- `render-corrections` prints one line to stderr for each slip it corrected, such as a key it does not take, and still writes the card.
- `render-refusals` refuses a bad intent, an unknown form, a missing highlight value and a bad option in one line.
- `render-hostile-input` refuses control, escape and bidi characters and every invisible format character (zero-width space, byte order mark, soft hyphen, tag characters) in any text but a zero width joiner between two emoji (ADR-050), and a number larger than 1e15 in magnitude, naming the key (ADR-024).
- `render-percent-fractions` refuses a percent measure sent as fractions of 1 (every value between -1 and 1 under a title or column that reads as a rate, share or a percent above 1), naming the column and no row value, and never multiplies a value (ADR-048).
- `render-hostile` never echoes control or escape characters from the intent to the terminal.
- `render-unshowable-highlight` refuses a highlight the form cannot colour: an x value on stacked-bar or slope, or any highlight on big-number.
- `render-incomplete` refuses a scatter or dumbbell with no row holding both numbers, and a slope with one period or with no series in both its first and last period.
- `render-long-labels` cuts long category, series and legend labels short with an ellipsis instead of refusing, and never prints two different categories alike.
- `render-legend-wrap` wraps a legend that does not fit one row onto more rows.
- `render-too-many-series` holds one rule: two different series never share a colour, except the rest group of a spotlight line chart (multi-line or slope naming one series), whose unnamed lines take four ordered context greys in series order and every line past the fourth the last of them (ADR-047). Anything else that would share a colour is refused: more series than five with none named (the shortest theme palette, so every theme refuses the same charts), more unnamed stacks or groups beside a highlight than its three neutrals, or more than one series named, since every named series takes the accent.

## How to get to it (user POV)

- Run `tiles render <intent.json> -o <out.png> [options]`.
- Pipe an intent: `cat intent.json | tiles render - -o out.png`.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.

- **File to PNG.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> render-file render crates/tiles-engine/tests/intents/bar.json -o '$OUT/card.png' --timings`. Exit 0; the transcript shows `bar form, obsidian theme, 1280x` (Obsidian is the default; a theme saved with `tiles theme` or `TILES_THEME` changes it, `--theme` beats both) and the engine load and card times.
- **Width and scale.** Run with name `render-wide` and `--width 900 --scale 1`. `files.txt` shows `900 x`.
- **Themes.** Render the line fixture with name `render-light` and `--theme light`. Exit 0; the transcript shows `line form, light theme` and `card.png` is a white card. With no `--theme` and no saved theme it shows `obsidian theme`: a graphite card with the line in purple.
- **Unknown form.** Write an intent with `"form": "pie"` to `$OUT/../pie.json` and render it with name `render-unknown`. Exit 1; the transcript shows `tiles: form: "pie" is not a form; use one of: column, bar, ...` and `card.png` does not exist.
- **Corrected slip.** Render the column fixture with `"colour": "week"` added at the top level, name `render-corrections`. Exit 0, `card.png` is the column card, and stderr shows `tiles: Tiles left out the keys it does not take: colour.`
- **Missing highlight.** Render the column fixture with its highlight changed to `W9`. Exit 1 and one line starting `tiles: highlight[0]:`.
- **Bad option.** Run with name `render-badopt` and `--theme blue`. Exit 2 and `tiles: --theme "blue" is not a theme; use obsidian, light, dark` (the build's themes, in list order) followed by the usage.
- **Hostile title.** Render an intent whose title contains `\u001b]52;c;aGk=\u0007`. Exit 1; the transcript shows `contains the control character U+001B` and no raw escape reaches the pane (`cat -v transcript.txt` shows no `^[`).
- **Hidden character.** Render the column fixture with its title changed to `Signups\u200b dipped`, name `render-invisible`. Exit 1 and `tiles: title: contains the invisible character U+200B; send plain text without control, escape, bidi or invisible characters`. Proved 2026-10-18.
- **Huge number.** Render the stacked-bar fixture with its first `accounts` value changed to `1.7e308`, name `render-huge`. Exit 1 and `tiles: data.rows[0].accounts: accounts holds a number larger than 1e15 in row 0; chart the values in a larger unit, such as millions`; `1e15` itself draws. Proved 2026-10-18.
- **Percent as fractions.** Render the line fixture with `"unit": "percent"`, its title changed to `Conversion fell to 3.2%` and its y values changed to fractions under 1, name `render-percent-fractions`. Exit 1 and one line starting `tiles: data.rows: every "` that ends `resend the values in points (4.1, not 0.041); if they are points under 1, say so in the title (0.4%, or 0.5 points)` and holds none of the row values. The same rows under the title `Conversion fell 0.5 points` exit 0 and draw them as written.
- **Highlight the form cannot colour.** Render the stacked-bar fixture with its highlight changed to `Q3`, name `render-unshowable-stacked`. Exit 1 and one line `tiles: highlight[0]: "Q3" occurs in no row of plan; name values exactly as they appear in the data, or drop highlight`. Render the big-number fixture with `"highlight": ["Q1 25"]` added, name `render-unshowable-kpi`: exit 0, the same card as without it (a big number ignores a highlight).
- **Pre-scaled rows.** Write a big-number intent titled `Monthly revenue rose to $128K` with fields `x` = `month`, `y` = `revenue` and rows `{"month": "Sep", "revenue": 120}`, `{"month": "Oct", "revenue": 128}` to `$OUT/../prescaled.json` and render it with name `render-prescaled`. Exit 1 and `tiles: data.rows: values look pre-scaled: the title says 128K but the rows hold it without its step; give full values (128000, not 128 for $128K); Tiles abbreviates them`. With 120000 and 128000 it draws, and so does the title `NPS among 50K customers held at 48` over rows of 40 to 62 ending at 48.
- **No complete row.** Write a scatter intent with fields `x` = `a`, `y` = `b` and rows `{"a": 1, "b": null}`, `{"a": null, "b": 2}` to `$OUT/../scatter-gap.json` and render it with name `render-incomplete-scatter`. Exit 1 and `tiles: data.rows: no row holds both an x and a y number; fill both columns in at least one row`. The dumbbell equivalent (`lo` and `hi` never both set) says `no row holds both a lo and a hi number`.
- **Slope without a pair.** Render a slope intent whose rows are `{"p": "2025", "s": "Search", "v": 1}` and `{"p": "2026", "s": "Email", "v": 2}` (fields `x` = `p`, `y` = `v`, `series` = `s`), name `render-incomplete-slope`. Exit 1 and `tiles: data.rows: no series has a y number in both the first and the last period; give at least one series a value in both periods`. With only the 2025 row it says `slope needs at least two periods in x; add rows for a second period`.
- **Long labels.** Render the bar fixture with its first two `account` values changed to `2026 Q3 Spring campaign NL returning - Search` and `2026 Q3 Spring campaign NL returning - Social` and its highlight removed, name `render-long-labels`. Exit 0. In `card.png` the two labels are cut from the middle: each keeps its start, an ellipsis, then `Search` or `Social`, so the two rows read differently. Render it again with the names `2026 Q3 Spring campaign NL returning - Search - all devices and platforms worldwide` and `... - Social - all devices and platforms worldwide`, name `render-long-window`: the differing end no longer fits after the start, so each label shows a window from its differing word, such as `…Search - all devices and…`, with an ellipsis only on a side where text is cut. A single long label with no look-alike keeps its start and ends in an ellipsis. Labels take at most about a third of the card width and the bars keep the rest.
- **Wrapped legend.** Render a grouped-bar intent with five series named `A rather long series name 0` to `4` over periods `Q1` and `Q2`, name `render-legend-wrap`. Exit 0; the legend runs onto a second row inside the card and the columns start below it, and each series has its own colour.
- **Too many series.** Render the same intent with six series (`... 0` to `5`), name `render-too-many-series`. Exit 1, no card, and `tiles: fields.series: 6 distinct values, at most 5 can be told apart; keep the top 4 and group the rest as Other, or pick another form`. Stacked-bar, multi-line and slope refuse the same way. Add `"highlight": ["A rather long series name 0"]` to a five-series grouped-bar, name `render-too-many-neutrals`: exit 1 and `tiles: fields.series: 4 series outside the highlight, at most 3 can be told apart; keep the top 2 beside the highlight and group the rest as Other, or pick another form`. A multi-line with nine series and a highlight naming one, name `render-spotlight`, exits 0 with the named line in the accent, the first four others in four different greys and the last four in the quietest of them. Name two of its series in the highlight, name `render-two-named`: exit 1 and `tiles: highlight: 2 series named on multi-line, only 1 can take the accent; name one, or drop highlight to give each series its own colour`; grouped-bar, stacked-bar and slope refuse the same way.

## Gotchas

- `tiles render` refuses an existing temporary name only by its process-specific suffix; a stale `.<name>.<pid>.tmp` from a killed run is left for the user to delete.
- `--frames` creates its directory; the evidence check is the 10 files, not the directory.
