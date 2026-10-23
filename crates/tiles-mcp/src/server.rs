//! The MCP server over stdio: newline-delimited JSON-RPC 2.0
//! (<https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#stdio>).
//!
//! Standard output carries protocol messages and nothing else; diagnostics go
//! to the log writer (standard error). The server listens on no port
//! (ADR-005). Every input line is bounded, every request is answered or
//! ignored without panicking, and a refused intent comes back as a tool
//! result with `isError`, so the model reads the fix and retries.
//!
//! A chart result carries text only, with no image and no
//! `structuredContent`. Everything in a tool result goes to the model
//! provider, and no MCP host draws from the result's image: the wrapper draws
//! from the card cache, the Claude Code mod from the saved file, and the text
//! route gives the path. Claude Code 2.1 also hands a tool's structured
//! content to the model in place of its text, which would drop the
//! placeholder block the model must copy. The text names the card id, its
//! cells and the PNG path for the add-ons, and leaves out the values of a card
//! drawn from rows Tiles fetched from Omni or a warehouse
//! ([`card::text_result`]).

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::cache::Cache;
use crate::card::{self, Fetch, Look, Painter, Refusal};
use crate::instructions::{self, Draw};
use crate::source::{Offer, Reach, Source};
use crate::{OMNI_FIELDS_TOOL, OMNI_TOPICS_TOOL, THEME_TOOL, TOOL_NAME, theme};

/// Protocol versions the server speaks, newest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Longest request line: the intent's 2 MB payload limit plus the envelope.
pub const MAX_LINE_BYTES: usize = tiles_intent::MAX_PAYLOAD_BYTES + 64 * 1024;

/// Browses Omni's topics and fields for the model (metadata only, never rows).
pub trait Browse {
    /// The topic listing as text.
    ///
    /// # Errors
    ///
    /// Omni's refusal, with no secret in it.
    fn topics(&self) -> Result<String, String>;
    /// One topic's fields as text.
    ///
    /// # Errors
    ///
    /// Omni's refusal, with no secret in it.
    fn fields(&self, topic: &str) -> Result<String, String>;
}

/// Everything a session of the server needs.
pub struct Server<'a> {
    /// The engine.
    pub painter: &'a dyn Painter,
    /// The rows source for Omni topics and warehouse queries.
    pub fetch: &'a Fetch<'a>,
    /// Omni browsing, when Omni is configured.
    pub browse: Option<&'a dyn Browse>,
    /// Each registered source that is configured, with what it offered
    /// when the session started: its tools and instruction sentences.
    pub sources: Vec<(&'a dyn Source, Offer)>,
    /// The card cache, or why it could not be opened.
    pub cache: Result<Cache, String>,
    /// The terminal geometry, read on every call so a resize applies.
    pub look: &'a dyn Fn() -> Look,
    /// How the card reaches the user.
    pub draw: Draw,
    /// Whether `tiles run`'s companion window shows the cards
    /// ([`crate::companion_from_env`]).
    pub companion: bool,
    /// The origins whose rows' values go to the model in a chart result:
    /// `Omni` with `TILES_OMNI_VALUES_TO_MODEL=1`, a source's origin with
    /// its own opt-in ([`Source::values_to_model`]).
    pub shared: Vec<String>,
    /// The environment, for the settings file and `TILES_THEME`.
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

#[allow(clippy::needless_pass_by_value)] // Every caller builds the result for this call.
fn response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[allow(clippy::needless_pass_by_value)] // Every caller formats the text for this call.
fn tool_text(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// The `tools/list` result for these forms: the chart tool, the theme tool,
/// the Omni tools when Omni is configured and each configured source's
/// tools. The add-ons publish the same `tiles_chart` entry.
#[must_use]
pub fn tools(forms: &[tiles_intent::schema::FormEntry], reach: &Reach) -> Value {
    let mut tools = vec![chart_tool(forms, reach), theme_tool()];
    let annotations = annotations(reach.fetches());
    if reach.omni {
        tools.push(json!({
            "name": OMNI_TOPICS_TOOL,
            "title": "List Omni topics",
            "description": "Lists the Omni topics Tiles can chart, with their labels.",
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} },
            "annotations": annotations,
        }));
        tools.push(json!({
            "name": OMNI_FIELDS_TOOL,
            "title": "List an Omni topic's fields",
            "description": "Lists one Omni topic's dimensions and measures with their types, the names data.omni.fields takes.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["topic"],
                "properties": { "topic": { "type": "string", "maxLength": 200 } },
            },
            "annotations": annotations,
        }));
    }
    for tool in reach.offers.iter().flat_map(|o| &o.tools) {
        tools.push(json!({
            "name": tool.name,
            "title": tool.title,
            "description": tool.description,
            "inputSchema": tool.input,
            "annotations": annotations,
        }));
    }
    json!({ "tools": tools })
}

/// The theme names, in list order.
fn theme_names() -> Vec<&'static str> {
    tiles_engine::Theme::all()
        .iter()
        .map(|t| t.name())
        .collect()
}

/// The `tiles_theme` tool: lists the themes, or sets one (ADR-021). It
/// writes only the local settings file and redraws cards already in the
/// cache, so it never reaches the network.
#[must_use]
pub fn theme_tool() -> Value {
    json!({
        "name": THEME_TOOL,
        "title": "List or set the card theme",
        "description": "Lists Tiles' card themes with the one in force marked, or, given name, sets \
            the theme every later card is drawn in and saves it on this machine. Use it when the \
            user asks to see or change the Tiles theme (/theme). With no name, show the list and \
            ask which theme they want. Pass the result text on to the user.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": { "name": { "type": "string", "enum": theme_names() } },
        },
        "annotations": {
            "title": "Tiles theme",
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false,
        },
    })
}

/// The `prompts/list` result: `theme`, which harnesses that turn MCP prompts
/// into commands offer as `/theme` or under the server's name.
#[must_use]
pub fn prompts() -> Value {
    json!({ "prompts": [{
        "name": "theme",
        "title": "Tiles theme",
        "description": "List Tiles' card themes, or switch to one",
        "arguments": [{
            "name": "name",
            "description": format!("{}; leave out to list them", theme_names().join(", ")),
            "required": false,
        }],
    }] })
}

/// The `/theme` request in words: what the model is asked to do with
/// `tiles_theme`.
#[must_use]
pub fn theme_request(name: Option<&str>) -> String {
    match name {
        Some(n) => format!(
            "Call the {THEME_TOOL} tool with name \"{n}\" and tell me what its result says."
        ),
        None => format!(
            "Call the {THEME_TOOL} tool with no arguments, show me the themes it lists, and ask \
             which one I want. When I pick one, call {THEME_TOOL} with that name and tell me what \
             its result says."
        ),
    }
}

fn prompt(params: &Value) -> Result<Value, (i64, String)> {
    if params.get("name").and_then(Value::as_str) != Some("theme") {
        return Err((-32602, "unknown prompt".into()));
    }
    let text = match params.get("arguments").and_then(|a| a.get("name")) {
        None | Some(Value::Null) => theme_request(None),
        Some(Value::String(n)) if n.trim().is_empty() => theme_request(None),
        Some(Value::String(n)) => {
            theme_request(Some(theme::parse(n).map_err(|e| (-32602, e))?.name()))
        }
        Some(_) => return Err((-32602, "name must be a theme name".into())),
    };
    Ok(json!({
        "description": "Tiles theme",
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}

/// The tools' hints. With Omni or a warehouse configured, `tiles_chart` and
/// the data tools can reach the user's data, so none is marked read-only
/// and closed-world: a client that honours the hints asks before running
/// one, however Tiles was installed.
fn annotations(fetches: bool) -> Value {
    json!({
        "title": "Tiles chart",
        "readOnlyHint": !fetches,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": fetches,
    })
}

/// The `tiles_chart` tool: its description lists the forms and their roles,
/// its input schema is generated from `tiles-intent`.
#[must_use]
pub fn chart_tool(forms: &[tiles_intent::schema::FormEntry], reach: &Reach) -> Value {
    let mut description = format!(
        "Draws a chart card inline in the user's terminal. Send v, form, title, claim, fields and \
         data as the call's own top-level arguments, never nested under another key. Use it to \
         answer any chart, graph or plot request instead of an artifact, web page, image file or \
         ASCII chart. Pick a form, name the column for each of its roles under fields, write the \
         finding as the title, and highlight only the values the title names. Tiles decides \
         sorting, colours, labels and axes. When the user asks for several metrics at once (a \
         dashboard, KPIs, an overview), make one call that draws them all as a dashboard: the \
         first card's keys at the top level, up to 3 more cards beside it under row, further rows under rows, and \
         dashboard {{title, note}} for the shared headline and data note. Give each scorecard \
         its recent periods (12 months), not one value. Title each card with a short metric \
         label (Monthly revenue) and no subtitle. Forms (roles; ? marks \
         optional):\n{}",
        tiles_intent::schema::forms_text(forms)
    );
    if reach.omni {
        description.push_str(
            "\nFor Omni data, pass data.omni {topic, fields, sorts?, limit?} and name those \
             fields under fields; Tiles fetches the rows.",
        );
    }
    for offer in &reach.offers {
        description.push('\n');
        description.push_str(offer.chart_line.trim());
    }
    json!({
        "name": TOOL_NAME,
        "title": "Draw a chart",
        "description": description,
        "inputSchema": with_row(tiles_intent::schema::intent_schema(forms)),
        "annotations": annotations(reach.fetches()),
    })
}

/// The intent schema with the dashboard keys: `row`, up to three more
/// intents beside this one; `rows`, further rows of 1 to 4 intents under
/// it; and `dashboard`, its title and shared note. Each card under `row`
/// and `rows` has the same keys; its copy of the schema drops the
/// descriptions the top level already gives, so the tool list stays short.
fn with_row(mut schema: Value) -> Value {
    fn undescribe(v: &mut Value) {
        match v {
            Value::Object(o) => {
                if o.get("description").is_some_and(Value::is_string) {
                    o.remove("description");
                }
                o.values_mut().for_each(undescribe);
            }
            Value::Array(a) => a.iter_mut().for_each(undescribe),
            _ => {}
        }
    }
    let mut card = schema.clone();
    undescribe(&mut card);
    if let Some(props) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        props.insert(
            "row".into(),
            json!({
                "type": "array",
                "minItems": 1,
                "maxItems": card::MAX_CARDS - 1,
                "items": card,
                "description": "Only for several cards at once, such as KPI scorecards: 1 to 3 more cards, each an object with the same keys as this one, drawn to the right of this card as one picture. Title every card, this one included, with a short metric label (Monthly revenue), not a sentence, and give none a subtitle. Cards in a row share one height with their parts lined up, each as wide as its chart needs, scorecards compact; a row too wide for the terminal goes on more lines",
            }),
        );
        props.insert(
            "rows".into(),
            json!({
                "type": "array",
                "minItems": 1,
                "maxItems": card::MAX_ROWS - 1,
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": card::MAX_CARDS,
                    "items": card,
                },
                "description": "For a dashboard: up to 3 more rows under the first (this card and row), each a list of 1 to 4 cards drawn left to right, at most 8 cards in all. Put scorecards together in the first row and wider charts (a line over months) in rows of their own below; Tiles moves a scorecard out of a row it shares with a chart",
            }),
        );
        props.insert(
            "dashboard".into(),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["title"],
                "properties": {
                    "title": { "type": "string", "maxLength": tiles_intent::MAX_TITLE_CHARS },
                    "note": { "type": "string", "maxLength": tiles_intent::MAX_SUBTITLE_CHARS },
                },
                "description": "For several cards: the dashboard's title (the headline, such as Business, last 12 months) and an optional note the cards share (data source, date range, Example data), drawn once above the cards",
            }),
        );
    }
    schema
}

/// Reads one line of at most `max` bytes. `Ok(None)` at end of input;
/// `Ok(Some(Err(())))` for a line that was too long, which is skipped.
fn read_line(input: &mut dyn BufRead, max: usize) -> io::Result<Option<Result<Vec<u8>, ()>>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let buf = input.fill_buf()?;
        if buf.is_empty() {
            return Ok(if line.is_empty() && !too_long {
                None
            } else if too_long {
                Some(Err(()))
            } else {
                Some(Ok(line))
            });
        }
        let (chunk, done) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (&buf[..i], Some(i + 1)),
            None => (buf, None),
        };
        if !too_long {
            if line.len() + chunk.len() > max {
                too_long = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let used = done.unwrap_or(buf.len());
        input.consume(used);
        if done.is_some() {
            return Ok(Some(if too_long { Err(()) } else { Ok(line) }));
        }
    }
}

impl Server<'_> {
    /// What this session can fetch from: Omni when it is configured, and
    /// each configured source.
    #[must_use]
    pub fn reach(&self) -> Reach {
        Reach {
            omni: self.browse.is_some(),
            offers: self.sources.iter().map(|(_, o)| o.clone()).collect(),
        }
    }

    /// The tools this server offers.
    #[must_use]
    pub fn tools(&self) -> Value {
        tools(&self.painter.forms(), &self.reach())
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": false },
                "prompts": { "listChanged": false },
            },
            "serverInfo": { "name": "tiles", "title": "Tiles", "version": tiles_intent::TILES_VERSION },
            "instructions": instructions::session(self.draw, self.companion, &self.reach()),
        })
    }

    fn chart(&self, arguments: &Value, log: &mut dyn Write) -> Value {
        let cache = match &self.cache {
            Ok(c) => c,
            Err(e) => return tool_text(format!("{TOOL_NAME} failed: {e}"), true),
        };
        let json = arguments.to_string();
        match card::make(&json, self.painter, self.fetch, cache, &(self.look)()) {
            Ok(card) => {
                let _ = writeln!(log, "tiles: drew card {} ({})", card.id, card.forms());
                tool_text(
                    card::text_result(&card, self.draw, self.companion, &self.shared),
                    false,
                )
            }
            Err(refusal) => {
                if let Refusal::Internal(e) = &refusal {
                    let _ = writeln!(log, "tiles: {}", tiles_term::sanitize::line(e, 300));
                }
                tool_text(
                    format!(
                        "{TOOL_NAME} refused: {}",
                        tiles_term::sanitize::line(&refusal.to_string(), 600)
                    ),
                    true,
                )
            }
        }
    }

    fn theme(&self, args: &Value) -> Result<Value, (i64, String)> {
        let name = match args.get("name") {
            None | Some(Value::Null) => return Ok(tool_text(theme::list(self.env), false)),
            Some(Value::String(n)) => n,
            Some(_) => return Err((-32602, "name must be a theme name".into())),
        };
        let chosen = match theme::parse(name) {
            Ok(t) => t,
            Err(e) => return Ok(tool_text(format!("{THEME_TOOL} refused: {e}"), true)),
        };
        Ok(
            match theme::set(
                chosen,
                self.env,
                self.painter,
                self.cache.as_ref().ok(),
                (self.look)().swapping_session(),
            ) {
                Ok(text) => tool_text(text, false),
                Err(e) => tool_text(
                    format!(
                        "{THEME_TOOL} failed: {}",
                        tiles_term::sanitize::line(&e, 400)
                    ),
                    true,
                ),
            },
        )
    }

    fn call(&self, params: &Value, log: &mut dyn Write) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let browse_result = |r: Result<String, String>| match r {
            Ok(text) => tool_text(tiles_term::sanitize::lines(&text), false),
            Err(e) => tool_text(
                format!("omni: {}", tiles_term::sanitize::line(&e, 600)),
                true,
            ),
        };
        match (name, self.browse) {
            (TOOL_NAME, _) if args.is_object() => Ok(self.chart(&args, log)),
            (TOOL_NAME, _) => Err((
                -32602,
                "arguments must be an object holding the chart's keys".into(),
            )),
            (THEME_TOOL, _) if args.is_object() => self.theme(&args),
            (THEME_TOOL, _) => Err((-32602, "arguments must be an object".into())),
            (OMNI_TOPICS_TOOL, Some(b)) => Ok(browse_result(b.topics())),
            (OMNI_FIELDS_TOOL, Some(b)) => match args.get("topic").and_then(Value::as_str) {
                Some(topic) => Ok(browse_result(b.fields(topic))),
                None => Err((-32602, "arguments need a topic".into())),
            },
            _ => match self
                .sources
                .iter()
                .find(|(_, offer)| offer.tools.iter().any(|t| t.name == name))
            {
                Some(_) if !args.is_object() => Err((-32602, "arguments must be an object".into())),
                Some((source, _)) => Ok(match source.tool(name, &args) {
                    Ok(text) => tool_text(tiles_term::sanitize::lines(&text), false),
                    Err(e) => tool_text(
                        format!("{}: {}", source.name(), tiles_term::sanitize::line(&e, 600)),
                        true,
                    ),
                }),
                None => Err((
                    -32602,
                    format!("unknown tool {}", tiles_term::sanitize::line(name, 60)),
                )),
            },
        }
    }

    /// The reply to one message, or `None` for a notification.
    pub fn handle(&self, message: &Value, log: &mut dyn Write) -> Option<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request we never sent, or garbage with an id.
            return message
                .get("id")
                .filter(|_| message.get("result").is_none() && message.get("error").is_none())
                .map(|id| error(id, -32600, "not a request"));
        };
        let id = message.get("id")?;
        if !(id.is_string() || id.is_number()) {
            return Some(error(
                &Value::Null,
                -32600,
                "id must be a string or a number",
            ));
        }
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        Some(match method {
            "initialize" => response(id, self.initialize(&params)),
            "ping" => response(id, json!({})),
            "tools/list" => response(id, self.tools()),
            "tools/call" => match self.call(&params, log) {
                Ok(result) => response(id, result),
                Err((code, message)) => error(id, code, &message),
            },
            "resources/list" => response(id, json!({ "resources": [] })),
            "prompts/list" => response(id, prompts()),
            "prompts/get" => match prompt(&params) {
                Ok(result) => response(id, result),
                Err((code, message)) => error(id, code, &message),
            },
            _ => error(id, -32601, "method not found"),
        })
    }

    /// Serves requests from `input` until it ends, writing only protocol
    /// messages to `output`.
    ///
    /// # Errors
    ///
    /// When reading the input or writing the output fails.
    pub fn serve(
        &self,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
        log: &mut dyn Write,
    ) -> io::Result<()> {
        while let Some(line) = read_line(input, MAX_LINE_BYTES)? {
            let reply = match line {
                Err(()) => Some(error(&Value::Null, -32600, "request line too long")),
                Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => None,
                Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(Value::Array(_)) => {
                        Some(error(&Value::Null, -32600, "batches are not supported"))
                    }
                    Ok(message) => self.handle(&message, log),
                    Err(_) => Some(error(&Value::Null, -32700, "parse error")),
                },
            };
            if let Some(reply) = reply {
                let mut text = reply.to_string();
                text.push('\n');
                output.write_all(text.as_bytes())?;
                output.flush()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
