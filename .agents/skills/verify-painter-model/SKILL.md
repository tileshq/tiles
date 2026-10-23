---
name: verify-painter-model
description: Model-check with TLC the wrapper painter's kitty motion route (crates/tiles-cli/src/wrap/paint.rs) - upload, entrance outbox, the Play state machine, early placement, settle, hand-over, resize and give_up against the harness's synchronized updates - in verify/formal/painter/Painter.tla. Use after any change to paint.rs's Play, Entrance, Early, upload, flush, settle, start, step, place_early, unplace_early, forget_placements or give_up, or when a card picture is left on screen or an entrance runs at the wrong time.
---

# Verify the painter model

Generated with `create-formal-verification-skill`. The model is `verify/formal/painter/Painter.tla`, its bounds `Painter.cfg`; `verify/formal/painter/README.md` states each property, what one model step stands for in the code, and what is left out. Read it first.

## Does the change touch the verified code?

Yes when the diff changes, in `crates/tiles-cli/src/wrap/paint.rs`, any of: `enum Play`, `struct Entrance`, `struct Early`, `Painter::upload`, `flush`, `settle`, `start`, `playing`, `step`, `early_copies`, `place_all_early`, `place_early`, `unplace_early`, `forget_placements`, `give_up`, `draw_placeholders`, or the `harness_sync` handling in `feed_painted` and `paint_now`. Each operator in `Painter.tla` names the function it models; a change to that function needs the operator changed to match before the check means anything. Pre-cut, sixel and the text route are not modelled.

## Run

```sh
sh verify/formal/painter/check.sh
```

It needs Java 11 or newer (`brew install --cask temurin` when `java -version` fails) and fetches the pinned `tla2tools.jar` (v1.7.4) through `scripts/fetch-tla2tools.sh` into `~/.cache/tiles-tla`. A pass ends `Model checking completed. No error has been found.` A failure prints `Error: Invariant <name> is violated.` or `Error: Temporal properties were violated.` and a numbered trace of states, each headed by the action that led to it.

## A failure

Read the trace as the code's calls: `Tick` is `Painter::tick`, `PaintNow(c)` a `Painter::feed` that ends at ground and paints card `c`, `Resize` is `Painter::resize`, `GiveUp` is `give_up` after a contained panic, `SendSome` and `SendRest` are `flush` writing part or the rest of an entrance, `Type`, `SyncBegin`, `SyncEnd`, `Arrive` and `Elapse` are the harness, the cache and the clock.

- A real bug: replay the trace in a unit test in `crates/tiles-cli/src/wrap/paint/tests.rs` with the `Fake` pictures (the test `a_broken_painter_removes_an_entrance_placed_early` is the pattern). If the test fails on the code, it is a bug: report it, fix the code in its own commit, keep the test.
- A modeling error: the test passes because the model lets a step happen that the code cannot take, or merges two calls the code keeps apart. Correct the operator to match the code and say what was wrong in the README. Never drop or weaken the property to make the check pass.

## An intended change

Change the operator that models the function you changed, then run the check. If a property no longer holds by design (a decision in `docs/card-rules.md` or an ADR), change it in the same commit, write why in the README, and keep a scratch break that still makes the new property fail.

Before trusting a change to the model, break it on a scratch copy the way a code bug would (for example make `Start` play whatever `hsync` says, or drop the `delete(early.image)` from `UnplaceEarly`) and see TLC report the property; then drop the scratch change.
