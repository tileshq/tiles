# ADR-045: The chart call's keys sit at the top level, a call nested under `intent` is read with a note, and a refusal names the keys Tiles left out

- Status: Accepted; amends [ADR-020](ADR-020-dashboards.md)'s list of corrections
- Date: 2026-10-21

## Context

In the v0.1.9 laptop test the first `tiles_chart` call from Claude Code was refused: the model sent its keys nested under one `intent` key, and the chart took two or three calls. The model's own words after the refusal were "The tool takes the fields at the top level, not nested under intent". Every refused call costs one more model turn through the company proxy, about 5 to 8 s for two refusals.

The tool's schema has no `intent` key, but the text the model reads called the call's arguments a "chart intent" in five places: the session instruction ("Fill its chart intent"), the tool description ("from a short chart intent"), the `row` and `rows` descriptions ("chart intents") and the error for arguments that are not an object. A refusal of the whole call also began `intent: not a valid intent`, which reads as a key named `intent`. Before this change the nested call lost its only key as unknown and was refused for a missing `form`, and the refusal did not say that Tiles had left `intent` out.

## Decision

- **Plain words.** The session instruction, the tool description, the schema's `v`, `row` and `rows` descriptions and the server's errors say the call takes v, form, title, claim, fields and data at its top level, never nested under another key, and call the extra cards under `row` and `rows` cards. No model-facing text says "intent". Type names, crate names and the ADRs keep the word.
- **A nested call is read.** Arguments that are exactly one key, `intent`, holding an object are read as that object (`tiles_intent::unnest`), at the call's top level (a whole dashboard nested under `intent` included) and for a card under `row` or `rows`. The result carries one sentence for it, as for every correction in ADR-020: "Tiles read the keys under intent as the call's own; send them at the top level, not under intent." The MCP server lifts each card before any other check, so a lifted card under `row` or `rows` meets the rule that it holds no `row`, `rows` or `dashboard` of its own; `parse_request` itself does not lift, so `tiles render` reads a file as written. An `intent` key beside other keys (`row`, `rows` and `dashboard` included), or holding something other than an object, is an unknown key as before.
- **A refusal names what Tiles left out.** A refusal after Tiles left out keys it does not take ends its problem with "and Tiles left out the keys it does not take: …", naming them, so a model that put the keys under another name (`chart`, say) sees which. A drawn card already named them in its note.
- **A refusal of the whole call names `arguments`.** A refusal about the call as a whole (not valid JSON for a chart, over the byte limit) names the key `arguments` and says "not a valid chart"; for a card under `row` or `rows` it names the card alone, such as `rows[0][0]: not a valid chart`. Its fix asks for v, form, title, claim, fields and data as keys of the object itself, not nested under another key. `tiles card` and `tiles render` use the same key for input over the limit or not UTF-8.

## Consequences

- A model that still nests under `intent` gets its card on the first call, with the note telling it where the keys go; one that nests under another name is refused once, with that name in the refusal.
- Other notes no longer say "intent": a version other than 1 reads "Tiles read v as 1." and a missing title "The call had no title, …".
- The wording has not been seen on the laptop yet; the tests below are the proof until the next laptop run.
- Tests: `unnest_lifts_only_a_lone_intent_object` and `a_refusal_names_the_keys_tiles_left_out` in `crates/tiles-intent/src/tests.rs`; `a_call_nested_under_intent_draws_the_flat_card_with_a_note`, `a_card_nested_under_intent_in_a_row_meets_the_row_guard`, `an_intent_key_beside_row_is_a_key_tiles_does_not_take`, `a_dropped_key_is_named_in_the_note_and_in_a_refusal` and `no_model_facing_text_calls_the_arguments_an_intent` in `crates/tiles-mcp/src/server/tests.rs`.
