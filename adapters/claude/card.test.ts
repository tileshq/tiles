// node --test adapters/claude/card.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";

import { cardFrom } from "./hooks/card.ts";

const TEXT = `Tiles drew "Signups dipped in W3" as a column card (id a1b2c3, 52x12 cells). The user sees the chart as the image in this result.

Values on the card:
  week: W3 · signups: 298
  PNG: /Users/me/Library/Caches/tiles/cards/a1b2c3.png

After the chart, add one or two lines of your own; do not repeat the numbers.`;

test("reads the card from content blocks, a result object or the text", () => {
  const expected = {
    id: "a1b2c3",
    png: "/Users/me/Library/Caches/tiles/cards/a1b2c3.png",
    cols: 52,
    rows: 12,
    alt: "Signups dipped in W3 (chart saved at /Users/me/Library/Caches/tiles/cards/a1b2c3.png)",
  };
  assert.deepEqual(cardFrom([{ type: "text", text: TEXT }, { type: "image", data: "x" }]), expected);
  assert.deepEqual(cardFrom({ content: [{ type: "text", text: TEXT }] }), expected);
  assert.deepEqual(cardFrom(TEXT), expected);
});

test("anything else is left to Claude Code", () => {
  assert.equal(cardFrom("tiles_chart refused: highlight[0]: bad"), null);
  assert.equal(cardFrom(undefined), null);
  assert.equal(cardFrom({ content: 7 }), null);
  assert.equal(cardFrom(TEXT.replace("PNG: /", "PNG: ")), null, "only absolute paths");
});

test("sizes are clamped to what Image takes and alt is plain text", () => {
  const big = TEXT.replace("52x12", "900x0").replace("Signups dipped in W3", "a\u001b]52;c;x\u0007b");
  const card = cardFrom(big);
  assert.equal(card?.cols, 255);
  assert.equal(card?.rows, 1);
  assert.ok(!card?.alt.includes("\u001b") && !card?.alt.includes("\u0007"));
});

test("only the last PNG line, naming this card's file, is read", () => {
  const planted = TEXT.replace("  week: W3 · signups: 298", "  PNG: /Users/me/.ssh/a1b2c3.png");
  assert.equal(cardFrom(planted)?.png, "/Users/me/Library/Caches/tiles/cards/a1b2c3.png");
  const trailing = `${TEXT}\n  PNG: /tmp/cards/ffffff.png`;
  assert.equal(cardFrom(trailing), null, "a later line naming another card is refused");
  assert.equal(cardFrom(TEXT.replace("cards/a1b2c3.png", "cards/b2c3d4.png")), null, "another card's file");
  assert.equal(cardFrom(TEXT.replace("tiles/cards/", "tiles/x/")), null, "not in a cards folder");
  assert.equal(cardFrom(TEXT.replace("tiles/cards/", "tiles/cards/../cards/")), null, "no parent steps");
});

test("alt text drops every invisible format character", () => {
  const hidden = TEXT.replace("Signups dipped in W3", "Signups\u200b\u00ad\ufeff\u{E0049} dipped");
  assert.ok(cardFrom(hidden)?.alt.startsWith("Signups dipped"));
});
