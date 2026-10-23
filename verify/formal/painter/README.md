# Painter model (TLA+, TLC)

Rerun: `sh verify/formal/painter/check.sh` (Java 11 or newer; the script fetches the pinned `tla2tools.jar` 1.7.4). About 20 seconds for both configs on the 2017 Mac.

`Painter.tla` models the wrapper painter's kitty motion route, `Route::KittyPlaceholders` with `Painter::with_motion`, in `crates/tiles-cli/src/wrap/paint.rs`: what the painter remembers about each card (`uploaded`, `sending`, `entrances` and their `Play` state, `shown`, `whole`, `early`, `broken`) and what kitty holds after the commands it sends (images and early placements). Each operator names the function it models.

## Properties

| Property | In one sentence | Harm when broken | Broken variant that fails it |
|---|---|---|---|
| `GaveUpCleanly` | A painter that gave up after a panic leaves no early placement on screen. | A stuck picture covers the harness's text, and a broken painter never draws again to remove it. | The code before the fix "remove an entrance placed early when the painter gives up" (finding F1, ADR-026): `give_up` deleted placement 2 of `card_image_id` instead of `early.image` |
| `RunsOutsideSync` | An entrance starts running only when the harness holds no synchronized update, or once `SYNC_WAIT` has passed since it was cued. | Kitty shows nothing new inside an update while the entrance's clock runs, so the entrance's first frames are lost. | `start` starts the clock (`Play::Playing`) whatever `harness_sync` says |
| `OnePicturePerCard` | Once a card's block is whole and its early placement handed over, kitty holds no entrance image and no backing for it. | Kitty would keep every played entrance and backing in memory for the rest of the session. | `unplace_early` without `delete(early.image)` |
| `PlacedPicturesExist` | The painter never places an image kitty does not hold. | Kitty refuses the placement and the card shows nothing until its block is whole. | `place_early` uses the entrance while its frames are still in the outbox |
| `EntranceEnds` | An entrance placed early is always handed over to its block in the end. | The early placement would float over the conversation for good. | `step` without the `SYNC_WAIT` fallback, with a harness that holds its update forever |

`check.sh` runs `Painter.cfg` (two cards, two-row blocks, the first four properties, 5,508 states) and `Liveness.cfg` (one card, a three-row block, `EntranceEnds` under `FairSpec`, about 200 states).

## What one step stands for

One action is one call into the painter, run to its end, or one thing outside it: `Tick` is `Painter::tick`; `PaintNow(c)` is a `Painter::feed` that ends at ground and paints card `c` (`paint`, then `step`); `Resize` is `Painter::resize` (`forget_placements`); `GiveUp` is `give_up` after a contained panic; `SendSome` and `SendRest` are `flush` writing part or the last of an entrance; `Type`, `SyncBegin`, `SyncEnd`, `Arrive` and `Elapse` are the harness typing a row, the harness's `CSI ? 2026 h` and `l`, the cache listing the card, and time passing `SYNC_WAIT` or the entrance's length.

## Bounds and assumptions

- Two cards with two-row blocks for safety, one card with three rows for liveness. Cards share no state in the model, so one card's paint per `PaintNow` covers the order `paint` takes them in.
- Every chunk ends at ground state, so `give_up`'s deletes go out at once; the tests `a_broken_painter_deletes_its_pictures_only_at_ground` and `random_output_and_resizes_never_panic_and_always_pass_through` cover the wait for ground.
- The block is typed from row 1 down, at one column, and stays on screen; a background colour is always known, so a backing is always placed.
- Time is abstract: `Elapse` may fire at any moment after an entrance is cued or starts, which covers every timing of `SYNC_WAIT` and `Entrance::length`.
- Weak fairness on `Progress` (ticks, paints, sends, time, typing) for `EntranceEnds`; the harness may hold or toggle its synchronized update forever.

## Left out

The pre-cut route, its strips and its entrance (ADR-030), which the paint tests cover; the sixel and text routes; `swap` (a card drawn again under its id); folded blocks, scrolling, half-typed tokens, blanking and write-back; `drop_gone`; the `SEND_BUDGET` split (one or two sends stand for any number); image ids, so `take_image_id` wrapping or meeting a `card_image_id` is not checked here; the unit test `no_painter_image_id_is_a_cards_image_id` covers it (finding F2).

## When to look again

A change to any function the operators name, listed in `.agents/skills/verify-painter-model/SKILL.md`.
