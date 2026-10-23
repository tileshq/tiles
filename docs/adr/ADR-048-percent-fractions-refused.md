# ADR-048: A percent measure given as fractions of 1 is refused

- Status: Accepted
- Date: 2026-10-21

## Context

[ADR-012](ADR-012-prototype-parity.md) takes percent values in points: 31 reads 31%. A model can still send a rate as a fraction of 1, as most warehouses and spreadsheets store it, so a conversion of 4.1% arrives as 0.041 and the card reads 0.041%. Tiles already refuses rows scaled down from the title's figure ([ADR-016](ADR-016-card-width-and-rows.md)) and draws a percent unit on a count of 1,000 or more as a count ([ADR-020](ADR-020-dashboards.md)); nothing caught the fraction. The owner chose to refuse it: Tiles names the key, tells the model to resend the values in points, and never multiplies them itself, since a measure that really sits under 1 point (an error rate of 0.3%, a change of -0.5 points) looks the same in the rows.

## Decision

- **When it applies.** The intent carries `unit: "percent"`, its rows are inline, and every number in a measure column the unit is written on (y, or `lo` and `hi` on a dumbbell) lies between -1 and 1 with at least one of them not 0. Each measure is checked on its own, so fractions in `lo` are refused even when `hi` holds points. Rows fetched from Omni or the warehouse are out of scope: the model cannot resend them.
- **What decides, never a row value.** The title and that measure's column name decide, read in this order:
  1. The word `point`, `points`, `pts`, `pp` or `bps`, alone or written onto its number (`0.4pp`, `30bps`), marks points of change, and the intent passes.
  2. A percent the title writes at 1 or less (`0.4%`, `under 1%`, `0.5 percent`) marks small points, and the intent passes.
  3. A percent the title writes above 1 (`fell to 3.2%`), a `%` sign, or the word `rate`, `rates`, `share`, `percent`, `percentage`, `pct` or `ratio` marks fractions, and the intent is refused.
  4. Anything else passes as given.
- **The refusal is one line** under the key `data.rows`, naming every refused measure column (`every "before" and "after" value` on a dumbbell) and no row value ([ADR-013](ADR-013-data-stays-local.md)): `data.rows: every "conversion" value lies between -1 and 1, so the rows look like fractions of 1 where the percent unit takes points; resend the values in points (4.1, not 0.041); if they are points under 1, say so in the title (0.4%, or 0.5 points)`. The MCP server returns it as an ordinary tool error (`isError`), as every other refusal.
- **Tiles never multiplies.** An intent that passes keeps its values exactly as sent.
- The `unit` description in the tool schema adds the example `(4.1 for 4.1%, not 0.041)`.

## Consequences

- A rate sent as fractions costs the model one retry instead of drawing a card a hundred times too small.
- A measure that really sits under 1 point and carries a rate or share word with no figure in the title is refused once; the fix tells the model to write its percent or "points" in the title, which then passes. Leaving the unit out also passes.
- A title that writes a percent of 1 or less passes even when its rows are fractions (`Refunds held under 0.5%` over 0.003 and 0.004); the title and rows then disagree in a way only the model can settle.
- A points word passes even beside a percent above 1, so `Conversion rose 0.8pp to 3.9%` passes over fractions; refusing it would also refuse a real point change such as `fell 0.5pp from 4.1%` over -0.5 to 0, and Tiles never multiplies either way.
