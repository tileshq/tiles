// Reads the card a tiles_chart result describes, for the mod's Image.
//
// The result's text says `(id a1b2c3, 52x12 cells)` and, last,
// `PNG: <cache>/cards/a1b2c3.png`; the MCP server's text result carries both. The output the ToolResult row
// carries may be the content blocks, a result object holding them, or the
// joined text, so all three are read.

export interface CardRef {
  id: string;
  png: string;
  cols: number;
  rows: number;
  alt: string;
}

function texts(output: unknown): string[] {
  if (typeof output === "string") return [output];
  if (Array.isArray(output)) return output.flatMap(texts);
  if (output && typeof output === "object") {
    const o = output as { type?: unknown; text?: unknown; content?: unknown };
    if (o.type === "text" && typeof o.text === "string") return [o.text];
    if (o.content !== undefined) return texts(o.content);
  }
  return [];
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

const clamp = (n: number) => Math.min(255, Math.max(1, Math.round(n)));

export function cardFrom(output: unknown): CardRef | null {
  const text = texts(output).join("\n");
  const cells = /\(id ([0-9a-f]{6}), (\d{1,4})x(\d{1,4}) cells\)/.exec(text);
  if (!cells) return null;
  // The server writes its own `PNG:` line after the value lines, so the
  // last one is the card's; it must name that card's file in a `cards`
  // folder, which no value line can fake.
  const png = Array.from(text.matchAll(/^\s*PNG: (\/.+\.png)\s*$/gm)).at(-1)?.[1];
  if (!png || !png.endsWith(`/cards/${cells[1]}.png`) || png.split("/").includes("..")) return null;
  const title = /^Tiles drew "(.*)" as a /m.exec(text)?.[1] ?? "Chart";
  return {
    id: cells[1],
    png,
    cols: clamp(Number(cells[2])),
    rows: clamp(Number(cells[3])),
    alt: clean(`${title} (chart saved at ${png})`),
  };
}
