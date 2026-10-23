// Contract tests for the Pi extension against a fake Pi API and a fake tiles
// binary: node --test adapters/pi/index.test.ts
import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import type { ToolSpec } from "./index.ts";

// Pi's loader aliases @earendil-works/pi-tui for every extension; here a stub
// Image stands in for it, rendering one line that names what it was given.
const stubTui = `export class Image {
  constructor(data, mime, theme, options) { Object.assign(this, { data, mime, options }); }
  render(width) { return [\`IMAGE \${this.mime} \${this.data} \${this.options.maxWidthCells} \${width}\`]; }
  invalidate() {}
}`;
register(
  `data:text/javascript,${encodeURIComponent(`export async function resolve(specifier, context, next) {
    if (specifier === "@earendil-works/pi-tui") {
      return { url: "data:text/javascript," + encodeURIComponent(${JSON.stringify(stubTui)}), shortCircuit: true };
    }
    return next(specifier, context);
  }`)}`,
);
const { default: tiles, OMNI_CHOICES, approveLookup, approveOmni, approveWarehouse, clean, fit, isCardPath, line, loadSpec, omniTopics, readCard, runTiles, standsDown, toolResult, warehouseLookup, warehouseTargets } =
  await import("./index.ts");

function fakeTiles(script: string): ToolSpec {
  const dir = mkdtempSync(join(tmpdir(), "tiles-pi-"));
  const bin = join(dir, "tiles");
  writeFileSync(bin, `#!/bin/sh\n${script}\n`);
  chmodSync(bin, 0o755);
  return { name: "tiles_chart", description: "d", inputSchema: { type: "object" }, instructions: "use tiles", tiles: bin, env: { TILES_DRAW: "image" } };
}

test("runTiles passes the intent on stdin and the draw mode in the environment", async () => {
  const spec = fakeTiles(`read -r intent; printf '{"ok":true,"card":"a1b2c3","png":"/x.png","title":"%s","form":"bar","cols":4,"rows":2,"text":"t"}' "$TILES_DRAW"`);
  const out = await runTiles(spec, { v: 1 });
  assert.equal(out.ok, true);
  assert.equal(out.ok && out.title, "image");
});

test("a failing or silent binary is an error result, never a throw", async () => {
  assert.deepEqual(await runTiles(fakeTiles("exit 1"), {}), { ok: false, error: "tiles gave no answer" });
  const missing = { ...fakeTiles(""), tiles: "/nonexistent/tiles" };
  const out = await runTiles(missing, {});
  assert.equal(out.ok, false);
});

test("toolResult gives the model text only and keeps the card for the renderer", () => {
  const r = toolResult({ ok: true, card: "a1b2c3", png: "/x.png", title: "T", form: "bar", cols: 4, rows: 2, text: "Tiles drew" });
  assert.deepEqual(r.content, [{ type: "text", text: "Tiles drew" }], "no image reaches the model provider");
  assert.deepEqual(r.details, { card: "a1b2c3", png: "/x.png", title: "T", cols: 4 });
  assert.equal(r.isError, false);
  const e = toolResult({ ok: false, error: "highlight[0]: \u001b]52;c;x\u0007 bad" });
  assert.equal(e.isError, true);
  assert.ok(!e.content[0].type.includes("\u001b") && !JSON.stringify(e.content).includes("\\u001b"));
});

test("clean removes escapes and bidi controls, keeps text", () => {
  assert.equal(clean("a\u001b[2Jb\u202ec\u0007 €"), "a[2Jbc €");
  assert.equal(clean("a\u200bb\u00adc\ufeffd\u2060e\u{E0049}f 👍"), "abcdef 👍", "every format character goes");
  assert.deepEqual(line("x\u001b]8;;y").render(3), ["x]8"]);
});

test("the factory registers the tool, its renderers and the instruction", async () => {
  const spec = fakeTiles(`printf '{"ok":false,"error":"form: nope"}'`);
  const registered: any[] = [];
  const handlers: Record<string, Function> = {};
  const pi = {
    registerTool: (t: any) => registered.push(t),
    registerCommand: () => {},
    on: (n: string, h: Function) => (handlers[n] = h),
  };
  tiles(pi, spec);
  assert.equal(registered.length, 1);
  const tool = registered[0];
  assert.equal(tool.name, "tiles_chart");
  assert.deepEqual(tool.parameters, { type: "object" });
  const theme = { fg: (_r: string, t: string) => t, bold: (t: string) => t };
  assert.deepEqual(tool.renderCall({ title: "Signups\u001b[2J", form: "bar" }, theme).render(80), ["tiles bar Signups[2J"]);
  const result = await tool.execute("id", { v: 1 }, undefined);
  assert.equal(result.isError, true);
  assert.deepEqual(tool.renderResult(result, {}, theme).render(80), ["form: nope"]);
  const ok = { content: [{ type: "text", text: "Drew card a1b2c3.\nmore" }], details: { png: "/c/a1b2c3.png" } };
  assert.deepEqual(tool.renderResult(ok, {}, theme).render(80), ["Drew card a1b2c3."], "a missing card file shows the result's first line");
  const event = { systemPromptOptions: { sections: {} as Record<string, string> } };
  handlers.before_agent_start(event);
  assert.equal(event.systemPromptOptions.sections.tiles, "use tiles");
});

test("/theme runs tiles theme and shows the user what it printed", async () => {
  const spec = fakeTiles(`if [ "$#" -eq 1 ]; then printf '* obsidian  graphite\\n  light\\033[2J  white\\n'; \
elif [ "$2" = neon ]; then echo 'tiles: "neon" is not a theme; use obsidian, light, dark' >&2; exit 1; \
else printf 'Theme set to %s by %s.\\n' "$2" "$TILES_DRAW"; fi`);
  const commands: Record<string, any> = {};
  tiles({ registerTool: () => {}, registerCommand: (n: string, c: any) => (commands[n] = c), on: () => {} }, spec);
  const shown: Array<[string, string | undefined]> = [];
  const ctx = { ui: { notify: (m: string, t?: string) => shown.push([m, t]) } };
  await commands.theme.handler("", ctx);
  await commands.theme.handler(" light ", ctx);
  await commands.theme.handler("neon", ctx);
  assert.deepEqual(shown, [
    ["* obsidian  graphite\n  light[2J  white", "info"],
    ["Theme set to light by image.", "info"],
    ['tiles: "neon" is not a theme; use obsidian, light, dark', "error"],
  ]);
  const missing = { ...fakeTiles(""), tiles: "/nonexistent/tiles" };
  const tilesGone: Record<string, any> = {};
  tiles({ registerTool: () => {}, registerCommand: (n: string, c: any) => (tilesGone[n] = c), on: () => {} }, missing);
  shown.length = 0;
  await tilesGone.theme.handler("dark", ctx);
  assert.equal(shown[0][1], "error");
  assert.match(shown[0][0], /^cannot run tiles/);
});

/** A theme that styles the way Pi 1.0's does: SGR codes around the text. */
const ansiTheme = {
  fg: (_r: string, t: string) => `\u001b[38;2;1;2;3m${t}\u001b[39m`,
  bold: (t: string) => `\u001b[1m${t}\u001b[22m`,
};

const visible = (s: string) => s.replace(/\u001b\[[0-9;:]*m/g, "");

test("the rows keep the theme's styles and clean only the user's text", async () => {
  const registered: any[] = [];
  tiles({ registerTool: (t: any) => registered.push(t), on: () => {}, registerCommand: () => {} }, fakeTiles(`printf '{"ok":false,"error":"form: nope"}'`));
  const tool = registered[0];
  const [call] = tool.renderCall({ title: "Signups\u001b[2J", form: "bar" }, ansiTheme).render(80);
  assert.equal(call, "\u001b[38;2;1;2;3m\u001b[1mtiles \u001b[22m\u001b[39m\u001b[38;2;1;2;3mbar\u001b[39m Signups[2J");
  const result = await tool.execute("id", {}, undefined);
  assert.equal(tool.renderResult(result, {}, ansiTheme).render(80)[0], "\u001b[38;2;1;2;3mform: nope\u001b[39m");
  const ok = { content: [{ type: "text", text: "t" }], details: { png: "/c/a1b2c3.png" } };
  assert.equal(tool.renderResult(ok, {}, ansiTheme).render(80)[0], "\u001b[38;2;1;2;3mt\u001b[39m");
});

test("rows are cut by visible width and keep closing their styles", () => {
  const registered: any[] = [];
  tiles({ registerTool: (t: any) => registered.push(t), on: () => {}, registerCommand: () => {} }, fakeTiles(""));
  const [cut] = registered[0].renderCall({ title: "Signups", form: "bar" }, ansiTheme).render(8);
  assert.equal(visible(cut), "tiles ba");
  assert.ok(cut.endsWith("\u001b[39m"));
  assert.equal(visible(fit("週報 sales", 5)), "週報 ");
  assert.equal(visible(fit("ab週", 3)), "ab");
  assert.equal(fit("\u001b[1mx\u001b[22m", 0), "\u001b[1m\u001b[22m");
});

test("renderResult draws the saved card alone, with no header line", async () => {
  const dir = mkdtempSync(join(tmpdir(), "tiles-pi-card-"));
  mkdirSync(join(dir, "cards"));
  const card = join(dir, "cards", "a1b2c3.png");
  writeFileSync(card, "png");
  const registered: any[] = [];
  tiles({ registerTool: (t: any) => registered.push(t), on: () => {}, registerCommand: () => {} }, fakeTiles(""));
  const theme = { fg: (_r: string, t: string) => t, bold: (t: string) => t };
  const ok = { content: [{ type: "text", text: "t" }], details: { png: card, cols: 52 } };
  assert.deepEqual(registered[0].renderResult(ok, {}, theme).render(300), [
    `IMAGE image/png ${Buffer.from("png").toString("base64")} 52 300`,
  ]);
  const elsewhere = join(dir, "a1b2c3.png");
  writeFileSync(elsewhere, "png");
  const other = { content: [{ type: "text", text: "t" }], details: { png: elsewhere, cols: 52 } };
  assert.deepEqual(registered[0].renderResult(other, {}, theme).render(300), ["t"], "only a card file is read");
});

test("where the wrapper draws, the result shows nothing and the card goes in the reply", async () => {
  const dir = mkdtempSync(join(tmpdir(), "tiles-pi-block-"));
  mkdirSync(join(dir, "cards"));
  const card = join(dir, "cards", "a1b2c3.png");
  writeFileSync(card, "png");
  const registered: any[] = [];
  const spec = { ...fakeTiles(""), env: { TILES_DRAW: "block" } };
  tiles({ registerTool: (t: any) => registered.push(t), on: () => {}, registerCommand: () => {} }, spec);
  const theme = { fg: (_r: string, t: string) => t, bold: (t: string) => t };
  const ok = { content: [{ type: "text", text: "Drew it.\n```tiles\n[tile:a1b2c3:1/2]\n```" }], details: { png: card, cols: 52 } };
  assert.deepEqual(registered[0].renderResult(ok, {}, theme).render(300), []);
  const failed = { content: [{ type: "text", text: "form: nope" }], isError: true };
  assert.deepEqual(registered[0].renderResult(failed, {}, theme).render(300), ["form: nope"], "a refusal still shows");
});

test("only a saved card's path is read", () => {
  assert.equal(isCardPath("/c/cards/a1b2c3.png"), true);
  assert.equal(isCardPath("cards/a1b2c3.png"), false);
  assert.equal(isCardPath("/c/cards/../../etc/a1b2c3.png.txt"), false);
  assert.equal(isCardPath("/home/u/.ssh/id_ed25519"), false);
  assert.equal(readCard("/nonexistent/cards/a1b2c3.png"), null);
});

test("loadSpec reads tool.json", () => {
  const dir = mkdtempSync(join(tmpdir(), "tiles-pi-spec-"));
  writeFileSync(join(dir, "tool.json"), JSON.stringify(fakeTiles("")));
  assert.equal(loadSpec(new URL(`file://${join(dir, "tool.json")}`)).name, "tiles_chart");
});

test("an installed copy stands down under tiles run", () => {
  const env = { TILES_RUN: "1", TILES_ADDONS: "/c/run/42/" };
  assert.equal(standsDown(env, "/home/u/.pi/agent/extensions/tiles/"), true);
  assert.equal(standsDown(env, "/c/run/42/pi/"), false);
  assert.equal(standsDown({}, "/home/u/.pi/agent/extensions/tiles/"), false);
});

const omniCard = (topic: unknown) => ({ title: "Revenue", form: "bar", data: { omni: { topic, fields: ["orders.revenue"] } } });

test("omniTopics finds every card that fetches from Omni, once each and cleaned", () => {
  assert.deepEqual(omniTopics({ title: "T", data: { rows: [{ a: 1 }] } }), [], "inline rows ask nothing");
  assert.deepEqual(omniTopics(null), []);
  assert.deepEqual(
    omniTopics({ ...omniCard("orders"), row: [omniCard("orders"), omniCard(7)], rows: [[omniCard("returns\u001b]0;x\u0007")], "junk"] }),
    ["orders", "Omni", "returns]0;x"],
  );
});

// The security review's S1: Pi has no tool approvals, so a Pi session with
// Omni configured fetched company rows without asking anyone.
test("an Omni-backed call asks the user first, and a refusal never runs tiles", async () => {
  const ran: string[] = [];
  const spec = fakeTiles(`echo ran >> "$TILES_LOG"; printf '{"ok":false,"error":"drawn"}'`);
  const log = join(mkdtempSync(join(tmpdir(), "tiles-pi-omni-")), "log");
  spec.env.TILES_LOG = log;
  const registered: any[] = [];
  const handlers: Record<string, Function> = {};
  tiles({ registerTool: (t: any) => registered.push(t), registerCommand: () => {}, on: (n: string, h: Function) => (handlers[n] = h) }, spec);
  const tool = registered[0];
  const answers: Array<string | undefined> = ["Decline", undefined, "Allow", "Allow for this session"];
  const ctx = {
    hasUI: true,
    ui: {
      select: async (title: string, options: string[]) => {
        ran.push(title);
        assert.deepEqual(options, OMNI_CHOICES);
        return answers.shift();
      },
    },
  };
  const ranTiles = () => {
    try {
      return readFileSync(log, "utf8").split("\n").filter(Boolean).length;
    } catch {
      return 0;
    }
  };
  const declined = await tool.execute("1", omniCard("orders"), undefined, undefined, ctx);
  assert.equal(declined.isError, true);
  assert.match(declined.content[0].text, /^The user declined fetching rows from Omni \(orders\)/);
  assert.equal(ranTiles(), 0, "tiles never ran");
  const dismissed = await tool.execute("2", omniCard("orders"), undefined, undefined, ctx);
  assert.equal(dismissed.isError, true, "a dismissed question is a no");
  await tool.execute("3", omniCard("orders"), undefined, undefined, ctx);
  assert.equal(ranTiles(), 1, "Allow runs this call");
  await tool.execute("4", omniCard("orders"), undefined, undefined, ctx);
  await tool.execute("5", omniCard("orders"), undefined, undefined, ctx);
  assert.equal(ranTiles(), 3, "Allow for this session stops asking");
  assert.equal(ran.length, 4);
  assert.equal(ran[0], "Tiles wants to fetch rows from Omni (orders) to draw a chart");
  handlers.session_start({ type: "session_start", reason: "new" });
  answers.push("Decline");
  await tool.execute("6", omniCard("orders"), undefined, undefined, ctx);
  assert.equal(ran.length, 5, "a new session asks again");
  await tool.execute("7", { title: "T", data: { rows: [{ a: 1 }] } }, undefined, undefined, ctx);
  assert.equal(ran.length, 5, "inline rows never ask");
});

test("with no one to ask, an Omni-backed call is refused unless tiles run allowed it", async () => {
  const session = { allowed: false };
  for (const ctx of [undefined, { hasUI: false, ui: { select: async () => "Allow" } }, { hasUI: true }]) {
    const refused = await approveOmni(omniCard("orders"), ctx, session, {});
    assert.match(refused ?? "", /no one to ask/, JSON.stringify(ctx));
  }
  assert.equal(await approveOmni(omniCard("orders"), undefined, session, { TILES_ALLOW_OMNI: "1" }), null);
  assert.equal(await approveOmni({ title: "T", data: { rows: [] } }, undefined, session, {}), null);
});

const warehouseCard = (target?: unknown) => ({
  title: "Revenue",
  form: "bar",
  data: { warehouse: { sql: "SELECT region, revenue FROM {{ ref('fct_revenue') }}", ...(target === undefined ? {} : { target }) } },
});

test("warehouseTargets finds every card that runs a warehouse query, once each and cleaned", () => {
  assert.deepEqual(warehouseTargets({ title: "T", data: { rows: [{ a: 1 }] } }), [], "inline rows ask nothing");
  assert.deepEqual(warehouseTargets(omniCard("orders")), [], "Omni asks its own question");
  assert.deepEqual(
    warehouseTargets({ ...warehouseCard("dev"), row: [warehouseCard("dev"), warehouseCard()], rows: [[warehouseCard("mr:12\u001b]0;x\u0007")]] }),
    ["dev build", "tables named in full", "mr:12]0;x build"],
  );
});

// ADR-034: a Pi session with a warehouse configured asks before every
// warehouse-backed chart, as it does for Omni.
test("a warehouse-backed call asks the user first, and a refusal never runs tiles", async () => {
  const spec = fakeTiles(`echo ran >> "$TILES_LOG"; printf '{"ok":false,"error":"drawn"}'`);
  const log = join(mkdtempSync(join(tmpdir(), "tiles-pi-warehouse-")), "log");
  spec.env.TILES_LOG = log;
  const registered: any[] = [];
  const handlers: Record<string, Function> = {};
  tiles({ registerTool: (t: any) => registered.push(t), registerCommand: () => {}, on: (n: string, h: Function) => (handlers[n] = h) }, spec);
  const tool = registered[0];
  const asked: string[] = [];
  const answers: Array<string | undefined> = ["Decline", "Allow for this session"];
  const ctx = {
    hasUI: true,
    ui: {
      select: async (title: string) => {
        asked.push(title);
        return answers.shift();
      },
    },
  };
  const ranTiles = () => {
    try {
      return readFileSync(log, "utf8").split("\n").filter(Boolean).length;
    } catch {
      return 0;
    }
  };
  const declined = await tool.execute("1", warehouseCard("dev"), undefined, undefined, ctx);
  assert.equal(declined.isError, true);
  assert.match(declined.content[0].text, /^The user declined running the warehouse query \(dev build\)/);
  assert.equal(ranTiles(), 0, "tiles never ran");
  await tool.execute("2", warehouseCard("dev"), undefined, undefined, ctx);
  await tool.execute("3", warehouseCard("prod"), undefined, undefined, ctx);
  assert.equal(ranTiles(), 2, "Allow for this session stops asking");
  assert.equal(asked[0], "Tiles wants to run a warehouse query (dev build) to draw a chart");
  assert.equal(asked.length, 2);
  handlers.session_start({ type: "session_start", reason: "new" });
  answers.push("Decline");
  await tool.execute("4", warehouseCard("dev"), undefined, undefined, ctx);
  assert.equal(asked.length, 3, "a new session asks again");
});

test("with no one to ask, a warehouse-backed call is refused unless tiles run allowed it", async () => {
  const session = { allowed: false };
  for (const ctx of [undefined, { hasUI: false, ui: { select: async () => "Allow" } }, { hasUI: true }]) {
    const refused = await approveWarehouse(warehouseCard("dev"), ctx, session, {});
    assert.match(refused ?? "", /tiles run --allow-warehouse pi/, JSON.stringify(ctx));
  }
  assert.equal(await approveWarehouse(warehouseCard("dev"), undefined, session, { TILES_ALLOW_WAREHOUSE: "1" }), null);
  assert.equal(await approveWarehouse(warehouseCard("dev"), undefined, session, { TILES_ALLOW_OMNI: "1" }), "Tiles asks before it runs a warehouse query, and this Pi session has no one to ask. Draw the chart from rows you already have, or tell the user to run Pi interactively or start it with tiles run --allow-warehouse pi.");
});

const lookups = [
  { name: "tiles_warehouse_models", description: "Lists dbt models", inputSchema: { type: "object", required: ["target"] } },
  { name: "tiles_warehouse_columns", description: "Lists columns", inputSchema: { type: "object" } },
];

test("warehouseLookup runs tiles warehouse with the arguments as argv, and refuses what the tool does not take", () => {
  assert.deepEqual(warehouseLookup("tiles_warehouse_models", { target: "dev", match: "fct_" }), {
    args: ["warehouse", "models", "--target", "dev", "fct_"],
    reads: "the dbt models of the dev build",
  });
  assert.deepEqual(warehouseLookup("tiles_warehouse_models", { target: "prod", match: "" }).args, ["warehouse", "models", "--target", "prod"]);
  assert.deepEqual(warehouseLookup("tiles_warehouse_columns", { model: "fct_payments; rm -rf ~", target: "dev" }).args, [
    "warehouse",
    "columns",
    "fct_payments; rm -rf ~",
    "--target",
    "dev",
  ]);
  assert.deepEqual(warehouseLookup("tiles_warehouse_columns", { table: "acme-prod.marts.fct_payments" }), {
    args: ["warehouse", "columns", "acme-prod.marts.fct_payments"],
    reads: "the columns of acme-prod.marts.fct_payments from BigQuery",
  });
  for (const [name, params, says] of [
    ["tiles_warehouse_models", {}, /^target is missing/],
    ["tiles_warehouse_columns", { model: "fct_payments" }, /^target is missing/],
    ["tiles_warehouse_columns", { table: "a.b.c", target: "dev" }, /^target goes with model/],
    ["tiles_warehouse_columns", {}, /^pass model/],
    ["tiles_warehouse_columns", { model: 7, target: "dev" }, /^model must be text/],
    ["tiles_warehouse_rows", { target: "dev" }, /^Tiles has no tool/],
  ] as const) {
    const found = warehouseLookup(name, params);
    assert.ok("error" in found && says.test(found.error), `${name} ${JSON.stringify(params)}: ${JSON.stringify(found)}`);
  }
});

test("the lookup tools exist only when tool.json lists them", () => {
  const names = (spec: ToolSpec) => {
    const registered: any[] = [];
    tiles({ registerTool: (t: any) => registered.push(t), registerCommand: () => {}, on: () => {} }, spec);
    return registered.map((t) => t.name);
  };
  assert.deepEqual(names(fakeTiles("")), ["tiles_chart"]);
  assert.deepEqual(names({ ...fakeTiles(""), tools: [] }), ["tiles_chart"]);
  assert.deepEqual(names({ ...fakeTiles(""), tools: lookups }), ["tiles_chart", "tiles_warehouse_models", "tiles_warehouse_columns"]);
});

// ADR-034, call 5: Pi reaches the warehouse only behind the extension's
// question, for the lookups as for a chart.
test("each lookup tool asks first, a refusal runs no tiles command, and Allow for this session covers charts too", async () => {
  const spec = {
    ...fakeTiles(`printf '%s|' "$@" >> "$TILES_LOG"; echo >> "$TILES_LOG"; if [ "$1" = card ]; then printf '{"ok":false,"error":"drawn"}'; else echo "listed $2"; fi`),
    tools: lookups,
  };
  const log = join(mkdtempSync(join(tmpdir(), "tiles-pi-lookup-")), "log");
  spec.env.TILES_LOG = log;
  const registered: Record<string, any> = {};
  const handlers: Record<string, Function> = {};
  tiles({ registerTool: (t: any) => (registered[t.name] = t), registerCommand: () => {}, on: (n: string, h: Function) => (handlers[n] = h) }, spec);
  const models = registered.tiles_warehouse_models;
  const columns = registered.tiles_warehouse_columns;
  assert.equal(models.description, "Lists dbt models");
  assert.deepEqual(models.parameters, { type: "object", required: ["target"] });
  const asked: string[] = [];
  const answers: Array<string | undefined> = ["Decline", undefined, "Allow", "Allow for this session"];
  const ctx = {
    hasUI: true,
    ui: {
      select: async (title: string, options: string[]) => {
        asked.push(title);
        assert.deepEqual(options, OMNI_CHOICES);
        return answers.shift();
      },
    },
  };
  const ran = () => {
    try {
      return readFileSync(log, "utf8").split("\n").filter(Boolean);
    } catch {
      return [];
    }
  };
  const declined = await models.execute("1", { target: "dev" }, undefined, undefined, ctx);
  assert.equal(declined.isError, true);
  assert.match(declined.content[0].text, /^The user declined reading the dbt models of the dev build\./);
  const dismissed = await columns.execute("2", { model: "fct_payments", target: "dev" }, undefined, undefined, ctx);
  assert.equal(dismissed.isError, true, "a dismissed question is a no");
  assert.deepEqual(ran(), [], "tiles never ran");
  const allowed = await columns.execute("3", { table: "acme-prod.marts.fct_payments" }, undefined, undefined, ctx);
  assert.deepEqual(allowed, { content: [{ type: "text", text: "listed columns" }], details: undefined, isError: false });
  const listed = await models.execute("4", { target: "dev", match: "fct_" }, undefined, undefined, ctx);
  assert.equal(listed.content[0].text, "listed models");
  await models.execute("5", { target: "prod" }, undefined, undefined, ctx);
  await registered.tiles_chart.execute("6", warehouseCard("dev"), undefined, undefined, ctx);
  assert.deepEqual(asked, [
    "Tiles wants to read the dbt models of the dev build",
    "Tiles wants to read the columns of fct_payments (dev build) from BigQuery",
    "Tiles wants to read the columns of acme-prod.marts.fct_payments from BigQuery",
    "Tiles wants to read the dbt models of the dev build",
  ]);
  assert.deepEqual(ran(), [
    "warehouse|columns|acme-prod.marts.fct_payments|",
    "warehouse|models|--target|dev|fct_|",
    "warehouse|models|--target|prod|",
    "card|-|",
  ], "Allow for this session stops asking for the lookups and the chart");
  const bad = await columns.execute("7", { model: "fct_payments" }, undefined, undefined, ctx);
  assert.match(bad.content[0].text, /^target is missing/);
  handlers.session_start({ type: "session_start", reason: "new" });
  answers.push("Decline");
  await models.execute("8", { target: "dev" }, undefined, undefined, ctx);
  assert.equal(asked.length, 5, "a new session asks again");
  assert.equal(ran().length, 4);
});

test("a lookup's refusal from tiles is a tool error, and with no one to ask a lookup is refused unless tiles run allowed it", async () => {
  const spec = { ...fakeTiles(`echo 'tiles: the dev manifest is not there yet' >&2; exit 1`), tools: lookups };
  const registered: Record<string, any> = {};
  tiles({ registerTool: (t: any) => (registered[t.name] = t), registerCommand: () => {}, on: () => {} }, spec);
  const ctx = { hasUI: true, ui: { select: async () => "Allow" } };
  const failed = await registered.tiles_warehouse_models.execute("1", { target: "dev" }, undefined, undefined, ctx);
  assert.deepEqual(failed.content, [{ type: "text", text: "tiles: the dev manifest is not there yet" }]);
  assert.equal(failed.isError, true);
  const session = { allowed: false };
  for (const ask of [undefined, { hasUI: false, ui: { select: async () => "Allow" } }, { hasUI: true }]) {
    const refused = await approveLookup("the dbt models of the dev build", ask, session, {});
    assert.match(refused ?? "", /no one to ask.*tiles run --allow-warehouse pi/, JSON.stringify(ask));
  }
  assert.equal(await approveLookup("the dbt models of the dev build", undefined, session, { TILES_ALLOW_WAREHOUSE: "1" }), null);
  assert.equal(await approveLookup("the dbt models of the dev build", undefined, session, { TILES_ALLOW_OMNI: "1" }) !== null, true);
});
