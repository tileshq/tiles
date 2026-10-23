# ADR-003: The model fills a short chart intent (form, fields, claim, highlight, title); Tiles owns the spec

- Status: Accepted; amended by [ADR-012](ADR-012-prototype-parity.md); the hosted intelligence reading ranges is superseded by [ADR-019](ADR-019-copying-without-company-data.md); for cards `tiles_chart` draws, its refusal of a highlight that names no category is superseded by [ADR-020](ADR-020-dashboards.md), which corrects the slips that draw no wrong number and says so in the result
- Date: 2026-10-17

## Context

In the prototype the model writes a Vega-Lite spec or names a form and passes its rows, and Tiles restyles the result. That costs tokens and fails in known ways: 53 of the 57 gallery specs name example values, so new rows draw the wrong highlight; the colour for a bad result came from a regular expression over the title's words; and research found models pass only 26% of combined answer-plus-chart checks (Text2Vis) even when their chart choice is defensible. Hex compiles its own compact chart language to Vega-Lite with five times fewer tokens. The chart sense (which form, what to colour, what to label) is the product, so it belongs in Tiles, not in each model's prompt.

## Decision

The MCP tool's only chart input is a **chart intent**: about 50 tokens plus the data reference.

```json
{
  "v": 1,
  "form": "bar",
  "title": "Signups dipped in W3",
  "claim": { "kind": "change", "tone": "bad" },
  "fields": { "x": "week", "y": "signups" },
  "highlight": ["W3"],
  "data": { "rows": [{ "week": "W1", "signups": 412 }, { "week": "W2", "signups": 431 }, { "week": "W3", "signups": 298 }] }
}
```

| Key | Meaning | Rules |
|---|---|---|
| `v` | Intent schema version | Required. Tiles accepts the current version and the one before it |
| `form` | The chart form, from the closed list Tiles publishes | Required. An unknown form is refused with the list of forms |
| `title` | The headline the reader sees: the finding in words | Required, at most 90 characters, plain text |
| `claim` | What kind of finding the title states, and its tone | `kind` from a closed list (`rank`, `change`, `compare`, `trend`, `share`, `distribution`, `relation`, `target`); `tone` is `good`, `bad` or `neutral`. The tone picks the accent colour, replacing the prototype's word list |
| `fields` | Which column fills each role the form declares | Roles per form, below. Every named column must exist in the data |
| `highlight` | The values the title names | Optional. Each value must occur in the data, or the intent is refused: no silent wrong highlight |
| `data` | Where the rows come from | `rows` inline (at most 10,000 rows), or an Omni source (ADR-008). Exactly one |
| `subtitle` | Optional second line | At most 120 characters |

Each form is a data file (ADR-002) that declares its roles and how they map to marks, so validation, the tool description and the drawing all come from the same definition. The form list the model sees is generated from the shipped form files. Roles in use:

| Roles | Forms that use them (examples; the engine round fixes the v0.1 list) |
|---|---|
| `x` (category or time) and `y` (number) | bar, column, line, area, lollipop, dot plot |
| `x`, `y` and `series` | grouped bar, multi-line, slope, stacked bar |
| `x`, `lo`, `hi` | dumbbell, range bar |
| `x`, `y`, optional `size` and `label` | scatter, bubble |
| `y` and optional `target` | big number, bullet |
| `x`, `y` with signed steps | waterfall |

Tiles, not the model, decides the rest from the form, the claim and the data: sort order, the accent on highlighted marks only, values on bars, the axis treatment, number formats, and the zero baseline for bars (`docs/playbook.md`).

### Validation

- Parsing rejects unknown keys and wrong types, so a typo fails loudly instead of being ignored.
- Numbers must be finite. Strings are plain text: control characters are refused (ADR-009).
- Every refusal names the key, the problem and the fix in one line, because the reader is a model that will retry.
- The MCP tool's input schema is generated from the Rust types in `tiles-intent`, so the schema the model sees and the parser cannot drift apart.
- A change of shape bumps `v`. The parser keeps accepting the previous version for one release, with a fixture for each.

### Later

- **Vega-Lite bridge:** a model-written Vega-Lite spec compiled in a sandboxed embedded engine with no network and no file access, its scene drawn by our renderer and card. It covers the long tail; the native forms stay the default.
- **Filters** (stage 2, ADR-001) arrive as a new key with a version bump.
- **Shape-only hosted intelligence** (ADR-005) can fill an intent from column types and ranges without seeing values.

## Consequences

- A model only has to pick a form, name the columns and state the finding, which weaker models do reliably.
- Every form is authored by us. Adding one is a form file plus its golden images, unless it needs a mark type the core lacks. The 57 prototype specs are the reference for which forms to build first.
- The intent is the format the eval set (ADR-006) and the preference data record.
