// Tiles for Pi (ADR-004): registers the tiles_chart tool and the /theme
// command, draws each card with pi-tui's Image component (kitty graphics or
// iTerm2 images) from the saved file, and adds the session instruction that
// makes the model choose the tile. Where `tiles run` gives the wrapper the
// drawing (TILES_DRAW=block: Warp, WezTerm, VS Code, herdr, sixel), the
// result shows nothing: the model copies the block into its reply and the
// card is drawn there, outside Pi's tool-result panel.
//
// Pi 1.0 API, from @earendil-works/pi-coding-agent 1.0.0:
// - docs/extensions.md: a default-exported factory receiving ExtensionAPI.
// - dist/core/extensions/types.d.ts: registerTool(ToolDefinition) with
//   execute(), renderCall() and renderResult(); before_agent_start exposes
//   systemPromptOptions.sections.
// - docs/extensions.md: a result's `content` is model-facing and its
//   `details` are for rendering. Image content would be drawn below the row
//   (dist/modes/interactive/components/tool-execution.js) but also sent to the
//   model provider, so the card travels in `details` as its file path and
//   renderResult draws it with pi-tui's Image, which the extension loader
//   aliases for every extension (dist/core/extensions/loader.js).
//
// - dist/core/extensions/types.d.ts: registerCommand(name, { description,
//   handler(args, ctx) }) adds a `/` command; ctx.ui.notify() shows a
//   message. Pi 1.0 has no `/theme` of its own (dist/core/slash-commands.js),
//   so the extension's `/theme` runs `tiles theme [name]` (ADR-021).
//
// - dist/core/extensions/types.d.ts: execute()'s fifth argument is the
//   extension context, whose ui.select() asks the user and whose hasUI says
//   whether anyone can answer (none in print and JSON modes); session_start
//   fires for a new, resumed or forked session. Pi has no tool
//   approvals of its own, so the extension asks before a call that fetches
//   rows from Omni (ADR-028) or reaches the warehouse (ADR-034), the way
//   the other harnesses' tool approval does.
//
// The chart logic stays in the tiles binary: execute() runs `tiles card -`
// with the intent on stdin. tool.json, written next to this file by
// `tiles run pi` or `tiles install pi`, holds the schema, description and
// instruction that binary generates, so the three cannot drift apart. While
// the warehouse is configured it also lists the lookup tools, which run
// `tiles warehouse models` and `tiles warehouse columns` with no shell.

import { spawn } from "node:child_process";
import { readFileSync, statSync } from "node:fs";

import { Image } from "@earendil-works/pi-tui";

/** What `tiles run` or `tiles install` writes beside this file. */
export interface ToolSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  instructions: string;
  tiles: string;
  env: Record<string, string>;
  /** The configured sources' lookup tools. */
  tools?: LookupSpec[];
}

/** A source's lookup tool, as `tiles` describes it in tool.json. */
export interface LookupSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/** The choices before a call that fetches rows from Omni (ADR-028). */
export const OMNI_CHOICES = ["Allow", "Allow for this session", "Decline"];

/** A call's cards in call order: the top card, the cards beside it under
 * `row` and those under `rows`. */
function cardsOf(params: unknown): unknown[] {
  const p = (params ?? {}) as { row?: unknown; rows?: unknown };
  const cards: unknown[] = [p];
  if (Array.isArray(p.row)) cards.push(...p.row);
  if (Array.isArray(p.rows)) p.rows.forEach((r) => Array.isArray(r) && cards.push(...r));
  return cards;
}

/** The Omni topics a call's cards fetch rows from, each once, in call order. */
export function omniTopics(params: unknown): string[] {
  const topics: string[] = [];
  for (const card of cardsOf(params)) {
    const omni = (card as { data?: { omni?: unknown } } | null)?.data?.omni;
    if (omni === null || typeof omni !== "object") continue;
    const topic = (omni as { topic?: unknown }).topic;
    const name = typeof topic === "string" ? clean(topic).slice(0, 60) : "Omni";
    if (!topics.includes(name)) topics.push(name);
  }
  return topics;
}

/** The dbt builds a call's warehouse queries read, each once, in call
 * order: the `target` of each card's `data.warehouse`, or "tables named in
 * full" for a query with none. */
export function warehouseTargets(params: unknown): string[] {
  const targets: string[] = [];
  for (const card of cardsOf(params)) {
    const warehouse = (card as { data?: { warehouse?: unknown } } | null)?.data?.warehouse;
    if (warehouse === null || typeof warehouse !== "object") continue;
    const target = (warehouse as { target?: unknown }).target;
    const name = typeof target === "string" ? `${clean(target).slice(0, 40)} build` : "tables named in full";
    if (!targets.includes(name)) targets.push(name);
  }
  return targets;
}

/** What the warehouse question names: what Tiles wants to do, the same
 * as a noun for a refusal, and the refusal when no one can answer. */
interface WarehouseAsk {
  wants: string;
  declined: string;
  unasked: string;
}

/** Asks the user before a call that reaches the warehouse, unless they
 * allowed it for this session or `tiles run --allow-warehouse` did. Returns
 * null when the call may go ahead, or the refusal the model reads. */
async function askWarehouse(
  ask: WarehouseAsk,
  ctx: AskContext | undefined,
  session: { allowed: boolean },
  env: Record<string, string | undefined>,
  signal?: AbortSignal,
): Promise<string | null> {
  if (session.allowed || env.TILES_ALLOW_WAREHOUSE === "1") return null;
  if (!ctx?.hasUI || typeof ctx.ui?.select !== "function") return ask.unasked;
  const pick = await ctx.ui.select(`Tiles wants to ${ask.wants}`, OMNI_CHOICES, { signal });
  if (pick === OMNI_CHOICES[1]) session.allowed = true;
  if (pick === OMNI_CHOICES[0] || pick === OMNI_CHOICES[1]) return null;
  return `The user declined ${ask.declined}. Do not try again; ask them how they want to go on.`;
}

/** Asks the user before a call that runs a warehouse query, as
 * askWarehouse does; a call with no warehouse query asks nothing. */
export async function approveWarehouse(
  params: unknown,
  ctx: AskContext | undefined,
  session: { allowed: boolean },
  env: Record<string, string | undefined>,
  signal?: AbortSignal,
): Promise<string | null> {
  const targets = warehouseTargets(params);
  if (targets.length === 0) return null;
  const ask = {
    wants: `run a warehouse query (${targets.join(", ")}) to draw a chart`,
    declined: `running the warehouse query (${targets.join(", ")})`,
    unasked:
      "Tiles asks before it runs a warehouse query, and this Pi session has no one to ask. Draw the chart from rows you already have, or tell the user to run Pi interactively or start it with tiles run --allow-warehouse pi.",
  };
  return askWarehouse(ask, ctx, session, env, signal);
}

/** The warehouse lookup tools the extension runs. */
export const MODELS_TOOL = "tiles_warehouse_models";
export const COLUMNS_TOOL = "tiles_warehouse_columns";

/** The `tiles` arguments a lookup tool call runs and what it reads, in
 * words for the question; or the refusal, worded as the MCP server words
 * it, for arguments the tool does not take. */
export function warehouseLookup(name: string, params: unknown): { args: string[]; reads: string } | { error: string } {
  const p = (params ?? {}) as Record<string, unknown>;
  for (const key of ["target", "match", "model", "table"]) {
    if (p[key] !== undefined && p[key] !== null && typeof p[key] !== "string") return { error: `${key} must be text` };
  }
  const text = (key: string) => (typeof p[key] === "string" ? (p[key] as string) : undefined);
  const shown = (s: string) => clean(s).slice(0, 60);
  const target = text("target");
  if (name === MODELS_TOOL) {
    if (target === undefined) {
      return { error: "target is missing: name the dbt build, prod, dev or mr:<iid>; if the user has not said which, ask them" };
    }
    const match = text("match");
    return {
      args: ["warehouse", "models", "--target", target, ...(match ? [match] : [])],
      reads: `the dbt models of the ${shown(target)} build`,
    };
  }
  if (name === COLUMNS_TOOL) {
    const model = text("model");
    const table = text("table");
    if (model !== undefined && table === undefined && !model.includes(".")) {
      if (target === undefined) {
        return {
          error: "target is missing: a dbt model is read from one build, prod, dev or mr:<iid>; if the user has not said which, ask them",
        };
      }
      return {
        args: ["warehouse", "columns", model, "--target", target],
        reads: `the columns of ${shown(model)} (${shown(target)} build) from BigQuery`,
      };
    }
    if (model === undefined && table !== undefined) {
      if (target !== undefined) return { error: "target goes with model, not with a table named in full" };
      return { args: ["warehouse", "columns", table], reads: `the columns of ${shown(table)} from BigQuery` };
    }
    return { error: "pass model (a dbt model name) with target, or table (project.dataset.table)" };
  }
  return { error: `Tiles has no tool ${shown(name)}` };
}

/** Asks the user before a lookup tool call, as askWarehouse does. */
export function approveLookup(
  reads: string,
  ctx: AskContext | undefined,
  session: { allowed: boolean },
  env: Record<string, string | undefined>,
  signal?: AbortSignal,
): Promise<string | null> {
  const ask = {
    wants: `read ${reads}`,
    declined: `reading ${reads}`,
    unasked:
      "Tiles asks before it reads the warehouse, and this Pi session has no one to ask. Tell the user to run Pi interactively or start it with tiles run --allow-warehouse pi.",
  };
  return askWarehouse(ask, ctx, session, env, signal);
}

/** The part of Pi's extension context the Omni question needs. */
export interface AskContext {
  hasUI?: boolean;
  ui?: { select?: (title: string, options: string[], opts?: { signal?: AbortSignal }) => Promise<string | undefined> };
}

/** Asks the user before a call that fetches rows from Omni, unless they
 * allowed it for this session or `tiles run --allow-omni` did. Returns null
 * when the call may go ahead, or the refusal the model reads. */
export async function approveOmni(
  params: unknown,
  ctx: AskContext | undefined,
  session: { allowed: boolean },
  env: Record<string, string | undefined>,
  signal?: AbortSignal,
): Promise<string | null> {
  const topics = omniTopics(params);
  if (topics.length === 0 || session.allowed || env.TILES_ALLOW_OMNI === "1") return null;
  if (!ctx?.hasUI || typeof ctx.ui?.select !== "function") {
    return "Tiles asks before it fetches rows from Omni, and this Pi session has no one to ask. Draw the chart from rows you already have, or tell the user to run Pi interactively or start it with tiles run --allow-omni pi.";
  }
  const pick = await ctx.ui.select(`Tiles wants to fetch rows from Omni (${topics.join(", ")}) to draw a chart`, OMNI_CHOICES, { signal });
  if (pick === OMNI_CHOICES[1]) session.allowed = true;
  if (pick === OMNI_CHOICES[0] || pick === OMNI_CHOICES[1]) return null;
  return `The user declined fetching rows from Omni (${topics.join(", ")}). Do not try again; ask them how they want to go on.`;
}

/** What `tiles card -` prints on success. */
export interface Card {
  ok: true;
  card: string;
  png: string;
  title: string;
  form: string;
  cols: number;
  rows: number;
  text: string;
}

export type CardOutcome = Card | { ok: false; error: string };

/** Most bytes read from `tiles card`. */
export const MAX_OUTPUT = 4 * 1024 * 1024;

/** Largest card file drawn; a real card is a small fraction of this. */
export const MAX_PNG = 16 * 1024 * 1024;

/** Whether `path` names a card Tiles saved: an absolute `.../cards/<id>.png`. */
export function isCardPath(path: string): boolean {
  return /^\/.*\/cards\/[0-9a-f]{6}\.png$/.test(path) && !path.includes("\u0000");
}

/** The card file as base64, or null when it is not a card path, gone or too big. */
export function readCard(path: string): string | null {
  if (!isCardPath(path)) return null;
  try {
    if (statSync(path).size > MAX_PNG) return null;
    return readFileSync(path).toString("base64");
  } catch {
    return null;
  }
}

/** Whether a code point can change what a terminal shows or does, or hide
 * text from the reader: C0 and C1 controls, DEL, the line and paragraph
 * separators, and every format character (Unicode category Cf: bidi
 * controls, zero-width characters, the byte order mark, the soft hyphen and
 * the invisible tag characters), the list tiles_intent::text shares. */
export function unsafe(c: number): boolean {
  return (
    c < 0x20 ||
    (c >= 0x7f && c <= 0x9f) ||
    c === 0x2028 ||
    c === 0x2029 ||
    (c >= 0xe0000 && c <= 0xe007f) ||
    /\p{Cf}/u.test(String.fromCodePoint(c))
  );
}

/** Text safe to show: every unsafe character removed. */
export function clean(text: string): string {
  return Array.from(text)
    .filter((ch) => !unsafe(ch.codePointAt(0) ?? 0))
    .join("");
}

export function loadSpec(url: URL = new URL("./tool.json", import.meta.url)): ToolSpec {
  return JSON.parse(readFileSync(url, "utf8")) as ToolSpec;
}

/** Runs `tiles card -` with the intent on stdin. */
export function runTiles(spec: ToolSpec, intent: unknown, signal?: AbortSignal): Promise<CardOutcome> {
  return new Promise((resolve) => {
    const child = spawn(spec.tiles, ["card", "-"], {
      env: { ...process.env, ...spec.env },
      stdio: ["pipe", "pipe", "pipe"],
      signal,
    });
    const chunks: Buffer[] = [];
    let size = 0;
    child.stdout.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size <= MAX_OUTPUT) chunks.push(chunk);
    });
    child.stderr.resume();
    child.on("error", (e) => resolve({ ok: false, error: `cannot run tiles: ${clean(e.message)}` }));
    child.on("close", () => {
      try {
        const parsed = JSON.parse(Buffer.concat(chunks).toString("utf8")) as CardOutcome;
        resolve(parsed.ok === true || parsed.ok === false ? parsed : { ok: false, error: "tiles gave no answer" });
      } catch {
        resolve({ ok: false, error: "tiles gave no answer" });
      }
    });
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(intent));
  });
}

/** Runs `tiles <args>`, with no shell, and gives what it printed, or its
 * refusal. */
export function runCommand(spec: ToolSpec, args: string[], signal?: AbortSignal): Promise<{ ok: boolean; text: string }> {
  return new Promise((resolve) => {
    const child = spawn(spec.tiles, args, {
      env: { ...process.env, ...spec.env },
      stdio: ["ignore", "pipe", "pipe"],
      signal,
    });
    const read = (stream: NodeJS.ReadableStream) => {
      const chunks: Buffer[] = [];
      let size = 0;
      stream.on("data", (chunk: Buffer) => {
        size += chunk.length;
        if (size <= MAX_OUTPUT) chunks.push(chunk);
      });
      return () => Buffer.concat(chunks).toString("utf8");
    };
    const out = read(child.stdout);
    const err = read(child.stderr);
    const lines = (text: string) => text.trim().split("\n").map(clean).join("\n");
    child.on("error", (e) => resolve({ ok: false, text: `cannot run tiles: ${clean(e.message)}` }));
    child.on("close", (code) =>
      resolve(code === 0 ? { ok: true, text: lines(out()) } : { ok: false, text: lines(err()) || "tiles gave no answer" }),
    );
  });
}

/** Runs `tiles theme`, or `tiles theme <name>` when `name` is not blank,
 * and gives what it printed: the list or the confirmation, or its refusal. */
export function runTheme(spec: ToolSpec, name: string): Promise<{ ok: boolean; text: string }> {
  const theme = name.trim();
  return runCommand(spec, theme ? ["theme", theme] : ["theme"]);
}

/** The Pi tool result of a lookup: what `tiles` printed, or its refusal. */
export function lookupResult(outcome: { ok: boolean; text: string }) {
  return { content: [{ type: "text" as const, text: outcome.text }], details: undefined, isError: !outcome.ok };
}

/** The Pi tool result for an outcome: the text the model reads, and the
 * card's file in `details` for renderResult. No image goes in `content`. */
export function toolResult(outcome: CardOutcome) {
  if (!outcome.ok) {
    return { content: [{ type: "text" as const, text: clean(outcome.error) }], details: undefined, isError: true };
  }
  return {
    content: [{ type: "text" as const, text: outcome.text }],
    details: { card: outcome.card, png: outcome.png, title: outcome.title, cols: outcome.cols },
    isError: false,
  };
}

/** The SGR codes a Pi theme wraps text in. */
const SGR = /(\u001b\[[0-9;:]*m)/;

/** `text` cut to `width` columns. The theme's SGR codes are kept, even past
 * the cut, so every style still closes; any other unsafe character is
 * dropped. A character outside ASCII counts two columns, so the row never
 * runs wider than pi-tui allows. */
export function fit(text: string, width: number): string {
  let out = "";
  let used = 0;
  text.split(SGR).forEach((part, i) => {
    if (i % 2 === 1) {
      out += part;
      return;
    }
    for (const ch of clean(part)) {
      const w = (ch.codePointAt(0) ?? 0) < 0x7f ? 1 : 2;
      if (used + w > width) {
        used = width;
        break;
      }
      out += ch;
      used += w;
    }
  });
  return out;
}

/** A one-line pi-tui component: render(width) returns the lines. The
 * caller cleans the untrusted parts before the theme styles them. */
export function line(text: string) {
  return {
    render: (width: number) => [fit(text, Math.max(0, width))],
    invalidate: () => {},
  };
}

type Component = { render: (width: number) => string[]; invalidate: () => void };

/** Components one under another. */
export function stack(parts: Component[]): Component {
  return {
    render: (width: number) => parts.flatMap((p) => p.render(width)),
    invalidate: () => parts.forEach((p) => p.invalidate()),
  };
}

type Theme = { fg: (role: string, text: string) => string; bold: (text: string) => string };

/** Whether this copy should stand down: under `tiles run pi` the launch loads
 * its own copy from TILES_ADDONS, so an installed one does nothing. */
export function standsDown(env: Record<string, string | undefined>, here: string): boolean {
  return env.TILES_RUN === "1" && typeof env.TILES_ADDONS === "string" && !here.startsWith(env.TILES_ADDONS);
}

export default function tiles(pi: any, spec?: ToolSpec) {
  if (spec === undefined) {
    if (standsDown(process.env, new URL(".", import.meta.url).pathname)) return;
    spec = loadSpec();
  }
  const omni = { allowed: false };
  const warehouse = { allowed: false };
  pi.registerTool({
    name: spec.name,
    label: "Tiles chart",
    description: spec.description,
    promptSnippet: `${spec.name}: draw a chart card inline in the terminal`,
    parameters: spec.inputSchema,
    async execute(_id: string, params: unknown, signal: AbortSignal | undefined, _update?: unknown, ctx?: AskContext) {
      const refused =
        (await approveOmni(params, ctx, omni, process.env, signal)) ??
        (await approveWarehouse(params, ctx, warehouse, process.env, signal));
      if (refused !== null) return toolResult({ ok: false, error: refused });
      return toolResult(await runTiles(spec, params, signal));
    },
    renderCall(args: { title?: unknown; form?: unknown }, theme: Theme) {
      const title = typeof args?.title === "string" ? args.title : "";
      const form = typeof args?.form === "string" ? args.form : "chart";
      return line(`${theme.fg("toolTitle", theme.bold("tiles "))}${theme.fg("muted", clean(form))} ${clean(title)}`);
    },
    renderResult(
      result: { content?: Array<{ type: string; text?: string }>; details?: { png?: unknown; cols?: unknown } },
      _o: unknown,
      theme: Theme,
    ) {
      const png = result?.details?.png;
      const first = clean((result?.content?.find((c) => c.type === "text")?.text ?? "").split("\n")[0] ?? "");
      if (typeof png !== "string" || !png) return line(theme.fg("warning", first));
      if (spec.env.TILES_DRAW === "block") return stack([]);
      const data = readCard(png);
      if (data === null) return line(theme.fg("dim", first));
      const cols = Math.min(255, Math.max(1, Math.round(Number(result.details?.cols) || 60)));
      return new Image(data, "image/png", { fallbackColor: (s: string) => theme.fg("toolOutput", s) }, { maxWidthCells: cols });
    },
  });
  for (const lookup of spec.tools ?? []) {
    if (lookup.name !== MODELS_TOOL && lookup.name !== COLUMNS_TOOL) continue;
    pi.registerTool({
      name: lookup.name,
      label: lookup.name === MODELS_TOOL ? "Tiles dbt models" : "Tiles columns",
      description: lookup.description,
      parameters: lookup.inputSchema,
      async execute(_id: string, params: unknown, signal: AbortSignal | undefined, _update?: unknown, ctx?: AskContext) {
        const found = warehouseLookup(lookup.name, params);
        if ("error" in found) return lookupResult({ ok: false, text: found.error });
        const refused = await approveLookup(found.reads, ctx, warehouse, process.env, signal);
        if (refused !== null) return lookupResult({ ok: false, text: refused });
        return lookupResult(await runCommand(spec, found.args, signal));
      },
      renderCall(args: unknown, theme: Theme) {
        const found = warehouseLookup(lookup.name, args);
        return line(`${theme.fg("toolTitle", theme.bold("tiles "))}${theme.fg("muted", "error" in found ? lookup.name : found.reads)}`);
      },
    });
  }
  pi.registerCommand("theme", {
    description: "List Tiles' card themes, or switch to one",
    async handler(args: string, ctx: { ui: { notify: (message: string, type?: "info" | "error") => void } }) {
      const result = await runTheme(spec, args ?? "");
      ctx.ui.notify(result.text, result.ok ? "info" : "error");
    },
  });
  // A new, resumed or forked session asks again.
  pi.on("session_start", () => {
    omni.allowed = false;
    warehouse.allowed = false;
  });
  pi.on("before_agent_start", (event: { systemPromptOptions: { sections: Record<string, string> } }) => {
    event.systemPromptOptions.sections.tiles = spec.instructions;
  });
}
