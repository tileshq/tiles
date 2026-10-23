//! Contract tests: the server against the MCP stdio rules and the tool's
//! promises, with a fake painter so no Skia is needed.
#![allow(clippy::needless_pass_by_value)] // Each test builds its values for one call.

use std::io::Cursor;
use std::time::Instant;

use serde_json::{Value, json};
use tiles_intent::source::Table;

use super::{Browse, MAX_LINE_BYTES, PROTOCOL_VERSIONS, Server};
use crate::cache::Cache;
use crate::card::tests::FakePainter;
use crate::card::{Fetched, Look, Query};
use crate::instructions::Draw;

struct FakeOmni;

impl Browse for FakeOmni {
    fn topics(&self) -> Result<String, String> {
        Ok("1 topic in Omni\n  orders  Orders\x1b]52;c;eA==\x07".into())
    }
    fn fields(&self, topic: &str) -> Result<String, String> {
        Err(format!("no topic {topic}"))
    }
}

/// The rows source a test server takes.
type FetchFn<'a> = dyn Fn(Query<'_>, Instant) -> Result<Fetched, String> + 'a;

fn no_omni(_: Query<'_>, _: Instant) -> Result<Fetched, String> {
    Err("Omni is not configured".into())
}

fn omni_rows(query: Query<'_>, _: Instant) -> Result<Fetched, String> {
    let Query::Omni(source) = query else {
        return Err("not an Omni query".into());
    };
    let rows: Vec<serde_json::Map<String, Value>> = serde_json::from_value(json!([
        { "orders.week": "W1", "orders.count": 3 },
        { "orders.week": "W2", "orders.count": 9 },
    ]))
    .unwrap();
    Table::from_json_rows(rows, Some(&source.fields), "omni.query")
        .map(Fetched::omni)
        .map_err(|e| e.to_string())
}

fn cache() -> Cache {
    let dir = std::env::temp_dir().join(format!("tiles-mcp-test-{}", std::process::id()));
    Cache::open(&dir).unwrap()
}

fn look() -> Look {
    Look::default()
}

fn with_server<T>(browse: bool, fetch: &FetchFn<'_>, f: impl FnOnce(&Server<'_>) -> T) -> T {
    with_settings(browse, fetch, false, f)
}

fn with_settings<T>(
    browse: bool,
    fetch: &FetchFn<'_>,
    omni_values: bool,
    f: impl FnOnce(&Server<'_>) -> T,
) -> T {
    with_draw(browse, fetch, omni_values, (Draw::Block, false), f)
}

fn with_draw<T>(
    browse: bool,
    fetch: &FetchFn<'_>,
    omni_values: bool,
    (draw, companion): (Draw, bool),
    f: impl FnOnce(&Server<'_>) -> T,
) -> T {
    let painter = FakePainter;
    let omni = FakeOmni;
    let server = Server {
        painter: &painter,
        fetch,
        browse: browse.then_some(&omni as &dyn Browse),
        sources: Vec::new(),
        cache: Ok(cache()),
        look: &look,
        draw,
        companion,
        shared: if omni_values {
            vec!["Omni".to_string()]
        } else {
            Vec::new()
        },
        env: &no_env,
    };
    f(&server)
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn ask(server: &Server<'_>, method: &str, params: Value) -> Value {
    let message = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
    server
        .handle(&message, &mut Vec::new())
        .expect("a request gets a reply")
}

const INTENT: &str = r#"{
  "v": 1, "form": "column", "title": "Signups dipped in W3",
  "claim": { "kind": "change", "tone": "bad" },
  "fields": { "x": "week", "y": "signups" }, "highlight": ["W3"],
  "data": { "rows": [ { "week": "W1", "signups": 412 }, { "week": "W3", "signups": 298 } ] }
}"#;

fn call(server: &Server<'_>, args: Value) -> Value {
    ask(
        server,
        "tools/call",
        json!({ "name": "tiles_chart", "arguments": args }),
    )
}

fn text_of(result: &Value) -> String {
    result["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn initialize_negotiates_the_version_and_carries_the_instruction() {
    with_server(false, &no_omni, |s| {
        let r = ask(
            s,
            "initialize",
            json!({ "protocolVersion": "2025-03-26", "capabilities": {} }),
        );
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 7);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "tiles");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert!(
            r["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("tiles_chart")
        );
        let r = ask(s, "initialize", json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    });
}

#[test]
fn the_text_route_instruction_names_the_companion_window_only_when_it_is_on() {
    for companion in [false, true] {
        with_draw(false, &no_omni, false, (Draw::Text, companion), |s| {
            let r = ask(s, "initialize", json!({}));
            let text = r["result"]["instructions"].as_str().unwrap();
            assert_eq!(
                text.contains("the Tiles window beside it shows each chart"),
                companion,
                "{text}"
            );
            assert_eq!(
                text.contains("give the PNG path from the result, so they can open or share it"),
                !companion,
                "{text}"
            );
        });
    }
}

#[test]
fn tools_list_offers_a_row_of_more_intents_without_repeating_descriptions() {
    with_server(false, &no_omni, |s| {
        let r = ask(s, "tools/list", json!({}));
        let schema = &r["result"]["tools"][0]["inputSchema"];
        let row = &schema["properties"]["row"];
        assert_eq!(row["type"], "array");
        assert_eq!(
            (row["minItems"].clone(), row["maxItems"].clone()),
            (json!(1), json!(3))
        );
        assert_eq!(row["items"]["required"], schema["required"]);
        assert_eq!(
            row["items"]["properties"]["form"]["enum"],
            json!(["column"])
        );
        assert!(
            row["items"]["properties"].get("row").is_none(),
            "one level only"
        );
        assert!(!row["items"].to_string().contains("description"));
        assert!(schema["properties"]["title"]["description"].is_string());
        let rows = &schema["properties"]["rows"];
        assert_eq!(rows["maxItems"], json!(3));
        assert_eq!(rows["items"]["maxItems"], json!(4));
        assert_eq!(rows["items"]["items"], row["items"]);
        let dashboard = &schema["properties"]["dashboard"];
        assert_eq!(dashboard["required"], json!(["title"]));
        assert_eq!(dashboard["additionalProperties"], json!(false));
        let description = r["result"]["tools"][0]["description"].as_str().unwrap();
        assert!(
            description.contains("several metrics at once"),
            "{description}"
        );
    });
}

#[test]
fn a_dashboard_call_returns_one_block_under_its_title() {
    with_server(false, &no_omni, |s| {
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        let card = |title: &str| {
            let mut v: Value = serde_json::from_str(INTENT).unwrap();
            v["title"] = json!(title);
            v
        };
        args["title"] = json!("Signups");
        args["row"] = json!([card("Orders")]);
        args["rows"] = json!([[card("Visits")]]);
        args["dashboard"] = json!({ "title": "Shop, last 3 weeks", "note": "Example data" });
        let r = call(s, args.clone());
        assert_eq!(r["result"]["isError"], false, "{}", text_of(&r));
        let text = text_of(&r);
        assert!(
            text.starts_with(
                "Tiles drew a dashboard \"Shop, last 3 weeks\" of 3 cards in 2 rows as one \
                 picture (id "
            ),
            "{text}"
        );
        // The lone card under the row stretches to the row's 81 columns.
        assert!(
            text.contains(
                ", 81x20 cells): \"Signups\" (column), \"Orders\" (column), \"Visits\" (column)."
            ),
            "{text}"
        );
        assert_eq!(text.matches("[tile:").count(), 20);
        // One block, inside one ```tiles fence (ADR-052).
        assert_eq!(text.matches("\n```tiles\n[tile:").count(), 1, "{text}");
        assert_eq!(text.matches("]\n```\n").count(), 1, "{text}");
        assert!(text.contains(" 01/20] Shop, last 3 weeks\n"), "{text}");
        for (key, bad, want) in [
            (
                "rows",
                json!([[]]),
                "rows[0]: a row holds 1 to 4 cards and this one has 0",
            ),
            (
                "rows",
                json!([card("A")]),
                "rows[0]: each row must be an array",
            ),
            (
                "rows",
                json!([[{ "v": 1 }]]),
                "rows[0][0]: not a valid chart",
            ),
            (
                "dashboard",
                json!({ "note": "Example data" }),
                "dashboard.title: a dashboard note needs a title",
            ),
            (
                "dashboard",
                json!({ "title": "A", "theme": "x" }),
                "dashboard.theme: is not a dashboard key",
            ),
            (
                "dashboard",
                json!({ "title": "A\u{1b}[2J" }),
                "dashboard.title: contains the control character U+001B",
            ),
            (
                "dashboard",
                json!({ "title": "A\u{200b}B" }),
                "dashboard.title: contains the invisible character U+200B",
            ),
            (
                "dashboard",
                json!("A"),
                "dashboard: dashboard must be an object",
            ),
        ] {
            let mut v = args.clone();
            v[key] = bad;
            let r = call(s, v);
            assert_eq!(r["result"]["isError"], true, "{key}");
            let got = text_of(&r);
            assert!(
                got.starts_with(&format!("tiles_chart refused: {want}")),
                "{got}"
            );
        }
        let mut v = args.clone();
        v["rows"] = json!([
            [card("A"), card("B"), card("C"), card("D")],
            [card("E"), card("F"), card("G")]
        ]);
        let got = text_of(&call(s, v));
        assert!(
            got.starts_with(
                "tiles_chart refused: rows: a dashboard holds at most 8 cards and this one has 9"
            ),
            "{got}"
        );
    });
}

#[test]
fn a_row_call_returns_one_block_for_both_cards() {
    with_server(false, &no_omni, |s| {
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        let mut second = args.clone();
        second["title"] = json!("Orders rose in W3");
        args["row"] = json!([second]);
        let r = call(s, args);
        assert_eq!(r["result"]["isError"], false);
        let text = text_of(&r);
        assert!(
            text.contains("Tiles drew 2 cards side by side in one row as one picture"),
            "{text}"
        );
        assert!(text.contains(
            ", 81x10 cells): \"Signups dipped in W3\" (column), \"Orders rose in W3\" (column)."
        ));
        assert_eq!(text.matches("[tile:").count(), 10);
        // One block, inside one ```tiles fence (ADR-052).
        assert_eq!(text.matches("\n```tiles\n[tile:").count(), 1, "{text}");
        assert_eq!(text.matches("]\n```\n").count(), 1, "{text}");
        assert!(text.contains(" 01/10] Signups dipped in W3 · Orders rose in W3\n"));
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        args["row"] = json!([{ "v": 1 }]);
        let r = call(s, args);
        assert_eq!(r["result"]["isError"], true);
        assert!(
            text_of(&r).starts_with("tiles_chart refused: row[0]: not a valid chart"),
            "{}",
            text_of(&r)
        );
    });
}

#[test]
fn tools_list_publishes_the_generated_schema() {
    with_server(false, &no_omni, |s| {
        let r = ask(s, "tools/list", json!({}));
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[1]["name"], "tiles_theme");
        assert_eq!(tools[1]["annotations"]["openWorldHint"], false);
        let tool = &tools[0];
        assert_eq!(tool["name"], "tiles_chart");
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(
            tool["inputSchema"]["properties"]["form"]["enum"],
            json!(["column"])
        );
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
        assert!(
            tool["description"]
                .as_str()
                .unwrap()
                .contains("- column (x, y)")
        );
    });
    with_server(true, &no_omni, |s| {
        let r = ask(s, "tools/list", json!({}));
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "tiles_chart",
                "tiles_theme",
                "tiles_omni_topics",
                "tiles_omni_fields"
            ]
        );
        for tool in r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["name"] != "tiles_theme")
        {
            assert_eq!(
                tool["annotations"]["readOnlyHint"], false,
                "{}",
                tool["name"]
            );
            assert_eq!(
                tool["annotations"]["openWorldHint"], true,
                "{}",
                tool["name"]
            );
        }
    });
}

#[test]
fn a_chart_call_returns_text_and_the_card_id_but_no_picture() {
    with_server(false, &no_omni, |s| {
        let r = call(s, serde_json::from_str(INTENT).unwrap());
        let result = &r["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"].as_array().unwrap().len(),
            1,
            "the model provider gets no picture of the card"
        );
        assert!(
            result.get("structuredContent").is_none(),
            "Claude Code would hide the text"
        );
        let text = text_of(&r);
        let at = text.find("(id ").unwrap() + 4;
        let id = &text[at..at + 6];
        assert!(tiles_term::block::is_card_id(id));
        let png = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("PNG: "))
            .unwrap();
        assert_eq!(std::fs::read(png).unwrap(), fake_default());
        assert!(text.contains(&format!("[tile:{id} 01/")));
        assert!(text.contains("W3: 298"));
    });
}

/// A result's text with its card id swapped for `id`, so two drawings of
/// one chart compare equal.
fn with_id(text: &str, id: &str) -> String {
    let at = text.find("(id ").unwrap() + 4;
    text.replace(&text[at..at + 6], id)
}

#[test]
fn a_call_nested_under_intent_draws_the_flat_card_with_a_note() {
    with_server(false, &no_omni, |s| {
        let flat = text_of(&call(s, serde_json::from_str(INTENT).unwrap()));
        let at = flat.find("(id ").unwrap() + 4;
        let id = &flat[at..at + 6];
        let args: Value = serde_json::from_str(INTENT).unwrap();
        let r = call(s, json!({ "intent": args }));
        assert_eq!(r["result"]["isError"], false, "{}", text_of(&r));
        let nested = with_id(&text_of(&r), id);
        let note = format!(" {}", tiles_intent::UNNESTED);
        assert!(nested.contains(&note), "{nested}");
        assert_eq!(nested.replacen(&note, "", 1), flat);
        // A dashboard nested whole, and a card nested under row, draw too.
        let mut board = args.clone();
        board["row"] = json!([{ "intent": args.clone() }]);
        board["dashboard"] = json!({ "title": "Shop" });
        let r = call(s, json!({ "intent": board }));
        assert_eq!(r["result"]["isError"], false, "{}", text_of(&r));
        let text = text_of(&r);
        assert!(text.contains("dashboard \"Shop\" of 2 cards"), "{text}");
        assert!(text.contains(&note), "{text}");
    });
}

#[test]
fn a_card_nested_under_intent_in_a_row_meets_the_row_guard() {
    with_server(false, &no_omni, |s| {
        let card: Value = serde_json::from_str(INTENT).unwrap();
        for nested in ["row", "rows", "dashboard"] {
            let mut inner = card.clone();
            inner[nested] = json!([card.clone()]);
            let mut args = card.clone();
            args["rows"] = json!([[inner.clone()]]);
            let flat = text_of(&call(s, args.clone()));
            args["rows"] = json!([[{ "intent": inner }]]);
            let r = call(s, args);
            assert_eq!(r["result"]["isError"], true);
            let want = format!(
                "tiles_chart refused: rows[0][0].{nested}: a card in a row cannot hold its own {nested}"
            );
            assert!(text_of(&r).starts_with(&want), "{}", text_of(&r));
            assert_eq!(text_of(&r), flat);
        }
    });
}

#[test]
fn a_percent_measure_in_fractions_is_a_tool_error_naming_no_value() {
    with_server(false, &no_omni, |s| {
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        args["title"] = json!("Signup rate dipped to 2.9% in W3");
        args["unit"] = json!("percent");
        args["data"]["rows"] = json!([
            { "week": "W1", "signups": 0.034 },
            { "week": "W3", "signups": 0.029 }
        ]);
        let r = call(s, args.clone());
        assert_eq!(r["result"]["isError"], true);
        let text = text_of(&r);
        assert!(
            text.starts_with(
                "tiles_chart refused: data.rows: every \"signups\" value lies between -1 and 1"
            ) && text.contains("resend the values in points (4.1, not 0.041)"),
            "{text}"
        );
        assert!(!text.contains("0.034") && !text.contains("0.029"), "{text}");
        args["data"]["rows"] = json!([
            { "week": "W1", "signups": 3.4 },
            { "week": "W3", "signups": 2.9 }
        ]);
        assert_eq!(call(s, args)["result"]["isError"], false);
    });
}

#[test]
fn an_intent_key_beside_row_is_a_key_tiles_does_not_take() {
    with_server(false, &no_omni, |s| {
        let card: Value = serde_json::from_str(INTENT).unwrap();
        let r = call(s, json!({ "intent": card.clone(), "row": [card.clone()] }));
        assert_eq!(r["result"]["isError"], true);
        let text = text_of(&r);
        assert!(
            text.starts_with("tiles_chart refused: arguments: not a valid chart: missing field")
                && text.contains("Tiles left out the keys it does not take: intent;"),
            "{text}"
        );
        assert!(!text.contains(tiles_intent::UNNESTED), "{text}");
        let mut args = card.clone();
        args["intent"] = card.clone();
        args["row"] = json!([card]);
        let text = text_of(&call(s, args));
        assert!(
            text.contains("Tiles left out the keys it does not take: intent."),
            "{text}"
        );
        assert!(!text.contains(tiles_intent::UNNESTED), "{text}");
    });
}

#[test]
fn a_dropped_key_is_named_in_the_note_and_in_a_refusal() {
    with_server(false, &no_omni, |s| {
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        args["colour"] = json!("red");
        let text = text_of(&call(s, args.clone()));
        assert!(
            text.contains("Tiles left out the keys it does not take: colour."),
            "{text}"
        );
        let r = call(s, json!({ "chart": args }));
        assert_eq!(r["result"]["isError"], true);
        let text = text_of(&r);
        assert!(
            text.starts_with("tiles_chart refused: arguments: not a valid chart: missing field")
                && text.contains("Tiles left out the keys it does not take: chart;"),
            "{text}"
        );
    });
}

#[test]
fn no_model_facing_text_calls_the_arguments_an_intent() {
    use crate::instructions::session;
    use crate::source::Reach;
    for reach in [Reach::default(), Reach::omni(true)] {
        for draw in [Draw::Block, Draw::Text, Draw::Image] {
            for companion in [false, true] {
                let text = session(draw, companion, &reach);
                assert!(!text.to_lowercase().contains("intent"), "{text}");
            }
        }
    }
    for browse in [false, true] {
        with_server(browse, &no_omni, |s| {
            let tools = ask(s, "tools/list", json!({})).to_string();
            assert!(!tools.to_lowercase().contains("intent"), "{tools}");
            let r = ask(
                s,
                "tools/call",
                json!({ "name": "tiles_chart", "arguments": "x" }),
            );
            let error = r["error"]["message"].as_str().unwrap();
            assert!(!error.contains("intent"), "{error}");
        });
    }
}

#[test]
fn a_refused_intent_is_a_tool_error_naming_the_key() {
    with_server(false, &no_omni, |s| {
        let mut intent: Value = serde_json::from_str(INTENT).unwrap();
        intent["highlight"] = json!(["W9"]);
        let r = call(s, intent);
        assert_eq!(r["result"]["isError"], true);
        let text = text_of(&r);
        assert!(
            text.starts_with("tiles_chart refused: highlight[0]"),
            "{text}"
        );
        let r = call(s, json!({ "v": 1 }));
        assert!(text_of(&r).contains("not a valid chart"));
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_chart", "arguments": "x" }),
        );
        assert_eq!(r["error"]["code"], -32602);
    });
}

#[test]
fn hostile_text_never_reaches_a_result_raw() {
    with_server(true, &no_omni, |s| {
        for (key, value) in [
            ("title", json!("Pwned\u{1b}]52;c;ZXZpbA==\u{7}")),
            ("subtitle", json!("\u{202e}gnp.exe")),
            ("fields", json!({ "x": "week\u{1b}[2J", "y": "signups" })),
            ("highlight", json!(["\u{9b}31m"])),
        ] {
            let mut intent: Value = serde_json::from_str(INTENT).unwrap();
            intent[key] = value;
            let r = call(s, intent);
            assert_eq!(r["result"]["isError"], true, "{key}");
            let text = text_of(&r);
            assert!(
                !text.chars().any(tiles_intent::text::is_unsafe_char),
                "{key}: {text:?}"
            );
        }
        let mut intent: Value = serde_json::from_str(INTENT).unwrap();
        intent["data"]["rows"][0]["week"] = json!("W1\u{1b}]8;;https://evil.test\u{1b}\\");
        let text = text_of(&call(s, intent));
        assert!(!text.chars().any(tiles_intent::text::is_unsafe_char));
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_omni_topics", "arguments": {} }),
        );
        assert!(!text_of(&r).contains('\u{1b}'));
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "evil\u{1b}[2J", "arguments": {} }),
        );
        assert!(!r["error"]["message"].as_str().unwrap().contains('\u{1b}'));
    });
}

#[test]
fn an_omni_intent_is_fetched_through_the_injected_source() {
    let intent = json!({
        "v": 1, "form": "column", "title": "Orders peaked in W2",
        "claim": { "kind": "rank", "tone": "good" },
        "fields": { "x": "orders.week", "y": "orders.count" }, "highlight": ["W2"],
        "data": { "omni": { "topic": "orders", "fields": ["orders.week", "orders.count"] } }
    });
    with_server(true, &omni_rows, |s| {
        let r = call(s, intent.clone());
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(r["result"]["content"].as_array().unwrap().len(), 1);
        let text = text_of(&r);
        assert!(
            !text.contains("W1") && !text.contains(": 9") && !text.contains(": 3"),
            "no Omni value reaches the model: {text}"
        );
        assert!(text.contains("Orders peaked in W2") && text.contains("[tile:"));
    });
    with_settings(true, &omni_rows, true, |s| {
        let r = call(s, intent.clone());
        assert!(text_of(&r).contains("W2: 9"), "the explicit opt-in");
    });
    with_server(false, &no_omni, |s| {
        let r = call(s, intent);
        assert_eq!(r["result"]["isError"], true);
        assert!(text_of(&r).contains("omni: Omni is not configured"));
    });
}

#[test]
fn omni_tools_exist_only_when_omni_is_configured() {
    with_server(false, &no_omni, |s| {
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_omni_topics", "arguments": {} }),
        );
        assert_eq!(r["error"]["code"], -32602);
    });
    with_server(true, &no_omni, |s| {
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_omni_fields", "arguments": { "topic": "x" } }),
        );
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(text_of(&r), "omni: no topic x");
    });
}

#[test]
fn stdout_carries_only_protocol_messages() {
    let long = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"x\":\"{}\"}}",
        "a".repeat(MAX_LINE_BYTES)
    );
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "",
        "not json",
        &long,
        r#"[{"jsonrpc":"2.0","id":2,"method":"ping"}]"#,
        r#"{"jsonrpc":"2.0","id":{"x":1},"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":"s","method":"nope"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
    ]
    .join("\n");
    let mut out = Vec::new();
    let mut errors = Vec::new();
    with_server(false, &no_omni, |s| {
        s.serve(&mut Cursor::new(input.into_bytes()), &mut out, &mut errors)
            .unwrap();
    });
    let replies: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("every stdout line is JSON"))
        .collect();
    let codes: Vec<Value> = replies.iter().map(|r| r["error"]["code"].clone()).collect();
    assert_eq!(
        codes,
        [
            Value::Null,
            json!(-32700),
            json!(-32600),
            json!(-32600),
            json!(-32600),
            json!(-32601),
            Value::Null
        ]
    );
    assert!(replies.iter().all(|r| r["jsonrpc"] == "2.0"));
    assert_eq!(replies.last().unwrap()["id"], 3);
}

#[test]
fn a_reply_without_a_method_is_ignored() {
    with_server(false, &no_omni, |s| {
        assert!(
            s.handle(
                &json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
                &mut Vec::new()
            )
            .is_none()
        );
        let r = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 1 }), &mut Vec::new())
            .unwrap();
        assert_eq!(r["error"]["code"], -32600);
    });
}

fn corpus() -> Vec<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_hostile_corpus_never_reaches_a_result_raw() {
    let plain = |r: &Value| {
        !r.to_string().contains("\\u001b")
            && text_of(r)
                .lines()
                .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char))
    };
    with_server(true, &no_omni, |s| {
        for e in corpus() {
            for (key, value) in [
                ("title", json!(e)),
                ("subtitle", json!(e)),
                ("form", json!(e)),
                ("fields", json!({ "x": e, "y": "signups" })),
                ("highlight", json!([e])),
            ] {
                let mut intent: Value = serde_json::from_str(INTENT).unwrap();
                intent[key] = value;
                let r = call(s, intent);
                assert!(plain(&r), "{key} {e:?}: {r}");
            }
            let mut intent: Value = serde_json::from_str(INTENT).unwrap();
            intent["data"]["rows"][0]["week"] = json!(e);
            assert!(plain(&call(s, intent)), "row {e:?}");
            let r = ask(
                s,
                "tools/call",
                json!({ "name": "tiles_omni_fields", "arguments": { "topic": e } }),
            );
            assert!(plain(&r), "topic {e:?}");
        }
    });
}

/// A server whose settings file and card cache sit in a fresh directory
/// `name`, inside the `tiles run` session `session`.
fn with_theme_server<T>(
    name: &str,
    session: Option<u64>,
    frames: u16,
    f: impl FnOnce(&Server<'_>, &std::path::Path) -> T,
) -> T {
    with_theme_fetch(name, session, frames, &no_omni, f)
}

/// [`with_theme_server`] with Omni rows from `fetch`.
fn with_theme_fetch<T>(
    name: &str,
    session: Option<u64>,
    frames: u16,
    fetch: &FetchFn<'_>,
    f: impl FnOnce(&Server<'_>, &std::path::Path) -> T,
) -> T {
    let dir = std::env::temp_dir().join(format!("tiles-theme-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let d = dir.display().to_string();
    let env = move |k: &str| (k == "TILES_CONFIG_DIR").then(|| d.clone());
    let look_env = env.clone();
    let look = move || Look {
        theme: crate::settings::theme(&look_env).0,
        session,
        frames,
        ..Look::default()
    };
    let painter = FakePainter;
    let server = Server {
        painter: &painter,
        fetch,
        browse: None,
        sources: Vec::new(),
        cache: Cache::open(&dir.join("cache")),
        look: &look,
        draw: Draw::Block,
        companion: false,
        shared: Vec::new(),
        env: &env,
    };
    let out = f(&server, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn theme_call(server: &Server<'_>, args: Value) -> Value {
    ask(
        server,
        "tools/call",
        json!({ "name": "tiles_theme", "arguments": args }),
    )
}

#[test]
fn the_theme_tool_lists_then_sets_and_the_next_card_follows() {
    with_theme_server("set", None, 0, |s, dir| {
        let list = text_of(&theme_call(s, json!({})));
        let first = crate::card::Theme::all()[0];
        assert!(list.starts_with(&format!("* {}", first.name())), "{list}");
        assert_eq!(
            first,
            crate::card::Theme::default(),
            "the default is listed first"
        );
        assert!(list.contains("the default"), "{list}");
        let r = theme_call(s, json!({ "name": "light" }));
        assert_eq!(r["result"]["isError"], false);
        let text = text_of(&r);
        assert!(text.starts_with("Theme set to light, saved in "), "{text}");
        assert!(
            text.contains("Cards already on screen keep their theme in this terminal; tiles open <id> shows an earlier card in light."),
            "{text}"
        );
        assert!(dir.join("config.json").exists());
        let list = text_of(&theme_call(s, json!({})));
        assert!(list.contains("* light"), "{list}");
        let card = call(s, serde_json::from_str(INTENT).unwrap());
        let png = text_of(&card)
            .lines()
            .find_map(|l| l.trim().strip_prefix("PNG: ").map(String::from))
            .unwrap();
        assert_eq!(std::fs::read(png).unwrap(), b"\x89PNG fake light");
        let r = theme_call(s, json!({ "name": "neon" }));
        assert_eq!(r["result"]["isError"], true);
        let names: Vec<&str> = crate::card::Theme::all().iter().map(|t| t.name()).collect();
        assert!(text_of(&r).contains(&format!("use {}", names.join(", "))));
        let r = theme_call(s, json!({ "name": 3 }));
        assert_eq!(r["error"]["code"], -32602);
    });
}

#[test]
fn a_theme_change_redraws_this_sessions_cards_under_their_ids() {
    with_theme_server("session", Some(0xabc), 15, |s, dir| {
        let card = call(s, serde_json::from_str(INTENT).unwrap());
        let text = text_of(&card);
        let id = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("PNG: "))
            .map(|p| {
                std::path::Path::new(p)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap();
        let cards = dir.join("cache/cards");
        assert_eq!(
            std::fs::read(cards.join(format!("{id}.png"))).unwrap(),
            fake_default()
        );
        let r = text_of(&theme_call(s, json!({ "name": other_theme() })));
        assert!(
            r.contains("The 1 card drawn earlier in this session switches in place."),
            "{r}"
        );
        assert_eq!(
            std::fs::read(cards.join(format!("{id}.png"))).unwrap(),
            fake_in(other_theme()).as_slice(),
            "the same id holds the new picture"
        );
        let meta: Value =
            serde_json::from_slice(&std::fs::read(cards.join(format!("{id}.json"))).unwrap())
                .unwrap();
        assert_eq!(meta["theme"], other_theme());
        assert_eq!(meta["frames"], 0);
        let again = text_of(&theme_call(s, json!({ "name": other_theme() })));
        assert!(
            !again.contains("switch"),
            "already {}: {again}",
            other_theme()
        );
    });
}

fn id_of(text: &str) -> String {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("PNG: "))
        .map(|p| {
            std::path::Path::new(p)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .unwrap()
}

const OMNI_INTENT: &str = r#"{
  "v": 1, "form": "column", "title": "Orders climbed",
  "claim": { "kind": "change", "tone": "good" },
  "fields": { "x": "orders.week", "y": "orders.count" },
  "data": { "omni": { "topic": "orders", "fields": ["orders.week", "orders.count"] } }
}"#;

// A dashboard whose typed card comes before its Omni card holds
// [None, Some(rows)]; the redraw hands the fetch the held rows in call
// order, so the swap comes from memory and never asks Omni again.
#[test]
fn a_mixed_dashboard_redraws_its_omni_card_from_memory() {
    let asked = std::cell::Cell::new(0);
    let fetch = |query: Query<'_>, at: Instant| {
        asked.set(asked.get() + 1);
        omni_rows(query, at)
    };
    with_theme_fetch("omni-mixed", Some(0xabc), 15, &fetch, |s, dir| {
        let mut args: Value = serde_json::from_str(INTENT).unwrap();
        args["rows"] = json!([[serde_json::from_str::<Value>(OMNI_INTENT).unwrap()]]);
        let r = call(s, args);
        assert_eq!(r["result"]["isError"], false, "{}", text_of(&r));
        let id = id_of(&text_of(&r));
        let cards = dir.join("cache/cards");
        let saved: Value = serde_json::from_str(
            &std::fs::read_to_string(cards.join(format!("{id}.call.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["omni"], json!([false, true]));
        assert_eq!(asked.get(), 1);

        let said = text_of(&theme_call(s, json!({ "name": other_theme() })));
        assert!(
            said.contains("The 1 card drawn earlier in this session switches in place."),
            "{said}"
        );
        assert_eq!(
            std::fs::read(cards.join(format!("{id}.png"))).unwrap(),
            fake_in(other_theme()).as_slice()
        );
        assert_eq!(asked.get(), 1, "the redraw never asks Omni again");
    });
}

// ADR-027: the rows behind an Omni card never reach the disk; the server
// that fetched them holds them and swaps the card in place from memory,
// and any other process shows the card as it was drawn and says why.
#[test]
fn an_omni_card_switches_theme_from_the_servers_memory_only() {
    with_theme_fetch("omni", Some(0xabc), 15, &omni_rows, |s, dir| {
        let id = id_of(&text_of(&call(
            s,
            serde_json::from_str(OMNI_INTENT).unwrap(),
        )));
        let cards = dir.join("cache/cards");
        let saved = std::fs::read_to_string(cards.join(format!("{id}.call.json"))).unwrap();
        let saved_json: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved_json["omni"], json!([true]), "{saved}");
        assert_eq!(saved_json["call"]["data"]["omni"]["topic"], "orders");
        assert!(saved_json.get("tables").is_none(), "{saved}");
        assert!(!saved.contains("W1") && !saved.contains("W2"), "{saved}");

        let r = text_of(&theme_call(s, json!({ "name": other_theme() })));
        assert!(
            r.contains("The 1 card drawn earlier in this session switches in place."),
            "{r}"
        );
        assert_eq!(
            std::fs::read(cards.join(format!("{id}.png"))).unwrap(),
            fake_in(other_theme()).as_slice()
        );

        // Another process, `tiles theme` or `tiles open`, opens the same
        // cache without the rows and asks for the default theme back.
        let back = crate::card::Theme::default();
        let other = Cache::open(&dir.join("cache")).unwrap();
        let painter = FakePainter;
        assert!(crate::card::needs_its_session(&other, &id));
        assert!(!crate::card::needs_its_session(
            s.cache.as_ref().unwrap(),
            &id
        ));
        let kept = crate::card::retheme(&painter, &other, 0xabc, back);
        assert_eq!(
            kept,
            crate::card::Rethemed {
                kept: 1,
                ..Default::default()
            }
        );
        let said = crate::theme::set(back, s.env, &painter, Some(&other), Some(0xabc)).unwrap();
        assert!(
            said.contains(
                "The 1 card drawn from Omni rows keeps its theme, since Tiles keeps those rows \
                 only in the memory of the session that fetched them."
            ),
            "{said}"
        );
        assert_eq!(
            std::fs::read(cards.join(format!("{id}.png"))).unwrap(),
            fake_in(other_theme()).as_slice(),
            "kept as drawn"
        );
        let shown = crate::card::shown(&id, &painter, &other, back).unwrap();
        assert_eq!(shown.id, id, "the card itself, no new card");
        assert_eq!(
            shown.theme,
            crate::card::Theme::from_name(other_theme()).unwrap()
        );
        assert_eq!(
            shown.note.as_deref(),
            Some(
                format!(
                    "Card {id} was drawn from Omni rows, which Tiles keeps only in the memory of \
                     the session that fetched them, so only that session can draw it again in \
                     another theme; ask for the chart again to see it in {}.",
                    back.name()
                )
                .as_str()
            )
        );
        let same = crate::card::shown(
            &id,
            &painter,
            &other,
            crate::card::Theme::from_name(other_theme()).unwrap(),
        )
        .unwrap();
        assert_eq!((same.id, same.note), (id.clone(), None));
        assert!(crate::card::reopen(&id, &painter, &other, &Look::default()).is_err());
    });
}

// A cache tampered with by hand, or a call file from before ADR-027 with
// its rows in it, still reaches the user only as plain text.
#[test]
fn the_hostile_corpus_in_a_saved_omni_card_reaches_tiles_open_plain() {
    let dir = std::env::temp_dir().join(format!("tiles-omni-hostile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = Cache::open(&dir).unwrap();
    let painter = FakePainter;
    let plain = |t: &str| !t.chars().any(tiles_intent::text::is_unsafe_char);
    for (n, e) in corpus().into_iter().enumerate() {
        let id = format!("{n:06x}");
        cache.write_card(&id, b"png").unwrap();
        cache
            .write_meta(
                &id,
                &json!({ "cols": 10, "rows": 4, "title": e, "theme": e }),
            )
            .unwrap();
        cache
            .write_call(
                &id,
                &json!({
                    "call": { "v": 1, "form": e, "title": e,
                              "data": { "omni": { "topic": e, "fields": [e] } } },
                    "tables": [{ "columns": [e], "rows": [[e]] }],
                    "look": { "term_cols": 80, "cell_px": [16, 32], "scale": 2.0 },
                }),
            )
            .unwrap();
        assert!(crate::card::needs_its_session(&cache, &id), "{e:?}");
        for &theme in crate::card::Theme::all() {
            match crate::card::shown(&id, &painter, &cache, theme) {
                Ok(shown) => {
                    let note = shown.note.unwrap_or_default();
                    assert!(plain(&note) && plain(shown.theme.name()), "{e:?}");
                }
                Err(t) => assert!(plain(&t), "{e:?}: {t:?}"),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// Warp and sixel cannot change a picture already on screen, so a session
// there, which has a token but plays no entrance, leaves its cards alone.
#[test]
fn a_theme_change_leaves_cards_alone_where_the_wrapper_cannot_swap_them() {
    with_theme_server("no-swap", Some(0xabc), 0, |s, dir| {
        let card = call(s, serde_json::from_str(INTENT).unwrap());
        let png = text_of(&card)
            .lines()
            .find_map(|l| l.trim().strip_prefix("PNG: ").map(String::from))
            .unwrap();
        let r = text_of(&theme_call(s, json!({ "name": "dark" })));
        assert!(
            r.contains("Cards already on screen keep their theme in this terminal"),
            "{r}"
        );
        assert_eq!(std::fs::read(png).unwrap(), fake_default());
        assert!(dir.join("config.json").exists());
    });
}

#[test]
fn the_theme_prompt_asks_for_the_tool() {
    with_server(false, &no_omni, |s| {
        let r = ask(s, "initialize", json!({}));
        assert!(r["result"]["capabilities"]["prompts"].is_object());
        let r = ask(s, "prompts/list", json!({}));
        assert_eq!(r["result"]["prompts"][0]["name"], "theme");
        assert_eq!(r["result"]["prompts"][0]["arguments"][0]["required"], false);
        let r = ask(
            s,
            "prompts/get",
            json!({ "name": "theme", "arguments": { "name": "dark" } }),
        );
        let text = r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(
            text.contains("tiles_theme tool with name \"dark\""),
            "{text}"
        );
        let r = ask(s, "prompts/get", json!({ "name": "theme" }));
        let text = r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("no arguments"), "{text}");
        let r = ask(
            s,
            "prompts/get",
            json!({ "name": "theme", "arguments": { "name": "neon\u{1b}[2J" } }),
        );
        assert_eq!(r["error"]["code"], -32602);
        assert!(!r.to_string().contains('\u{1b}'));
        let r = ask(s, "prompts/get", json!({ "name": "other" }));
        assert_eq!(r["error"]["code"], -32602);
    });
}

#[test]
fn a_card_for_a_window_is_its_own_picture_unless_the_theme_changed() {
    with_theme_server("window", None, 0, |s, dir| {
        let text = text_of(&call(s, serde_json::from_str(INTENT).unwrap()));
        let png = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("PNG: "))
            .unwrap();
        let id = std::path::Path::new(png)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            text.contains(&format!(
                "The user can open this card in a window of its own by running `tiles open {id}`."
            )),
            cfg!(target_os = "macos"),
            "{text}"
        );
        let cache = Cache::open(&dir.join("cache")).unwrap();
        let painter = FakePainter;
        let same =
            crate::card::shown(&id, &painter, &cache, crate::card::Theme::default()).unwrap();
        assert_eq!(same.id, id, "drawn in the theme in force: the card itself");
        assert_eq!(same.path.display().to_string(), png);
        let light = crate::card::shown(
            &id,
            &painter,
            &cache,
            crate::card::Theme::from_name("light").unwrap(),
        )
        .unwrap();
        assert_ne!(light.id, id, "another theme: a new card");
        assert_eq!(std::fs::read(&light.path).unwrap(), b"\x89PNG fake light");
        assert_eq!(light.title, same.title);
        assert!(
            crate::card::shown(
                "../x",
                &painter,
                &cache,
                crate::card::Theme::from_name("light").unwrap()
            )
            .is_err()
        );
        assert!(
            crate::card::shown(
                "abcdef",
                &painter,
                &cache,
                crate::card::Theme::from_name("light").unwrap()
            )
            .is_err()
        );
    });
}

/// A row value only the warehouse knows; no result, log or call file may
/// hold it.
const PLANTED_ROW: &str = "planted_row_value_Q9x";

/// A warehouse source for the server's tests: two tools whose text comes
/// from `said`, and no network.
struct FakeWarehouse {
    said: Result<String, String>,
}

impl crate::source::Source for FakeWarehouse {
    fn name(&self) -> &'static str {
        "warehouse"
    }
    fn usage(&self) -> &'static str {
        "tiles warehouse <...>"
    }
    fn doctor(&self) -> Vec<String> {
        Vec::new()
    }
    fn command(&self, _: &[String]) -> Result<String, String> {
        Ok(String::new())
    }
    fn tool(&self, _: &str, _: &Value) -> Result<String, String> {
        self.said.clone()
    }
}

fn warehouse_offer() -> crate::source::Offer {
    let tool = |name| crate::source::Tool {
        name,
        title: "t",
        description: "d",
        input: json!({ "type": "object", "properties": {} }),
    };
    crate::source::Offer {
        name: "warehouse",
        origin: "BigQuery",
        tools: vec![
            tool("tiles_warehouse_models"),
            tool("tiles_warehouse_columns"),
        ],
        instruction: "Pass data.warehouse with a SELECT.".into(),
        chart_line: "For warehouse data, pass data.warehouse {sql, target?, limit?}.".into(),
        tool_timeout: std::time::Duration::from_secs(300),
    }
}

/// Warehouse rows holding [`PLANTED_ROW`], with a note on the build.
fn warehouse_rows(query: Query<'_>, _: Instant) -> Result<Fetched, String> {
    let Query::Warehouse(_) = query else {
        return Err("not a warehouse query".into());
    };
    let rows: Vec<serde_json::Map<String, Value>> = serde_json::from_value(json!([
        { "region": PLANTED_ROW, "revenue": 4_217 },
        { "region": "North", "revenue": 3_100 },
    ]))
    .unwrap();
    Ok(Fetched {
        table: Table::from_json_rows(rows, None, "warehouse.query").map_err(|e| e.to_string())?,
        origin: "BigQuery".into(),
        notes: vec!["Read fct_revenue from the dev build.".into()],
        label: Some("dev build · personal_ada_lovelace.fct_revenue".into()),
    })
}

const WAREHOUSE_INTENT: &str = r#"{
  "v": 1, "form": "column", "title": "Revenue by region",
  "claim": { "kind": "rank", "tone": "good" },
  "fields": { "x": "region", "y": "revenue" },
  "data": { "warehouse": { "sql": "SELECT region, revenue FROM {{ ref('fct_revenue') }}", "target": "dev" } }
}"#;

/// A server with the warehouse configured, its rows from `fetch`, the
/// values shared when `shared` says so, and its cache in a fresh directory
/// `name`.
fn with_warehouse<T>(
    name: &str,
    source: &FakeWarehouse,
    fetch: &FetchFn<'_>,
    shared: bool,
    f: impl FnOnce(&Server<'_>, &std::path::Path) -> T,
) -> T {
    let dir = std::env::temp_dir().join(format!("tiles-wh-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let painter = FakePainter;
    let server = Server {
        painter: &painter,
        fetch,
        browse: None,
        sources: vec![(source as &dyn crate::source::Source, warehouse_offer())],
        cache: Cache::open(&dir),
        look: &look,
        draw: Draw::Block,
        companion: false,
        shared: if shared {
            vec!["BigQuery".to_string()]
        } else {
            Vec::new()
        },
        env: &no_env,
    };
    let out = f(&server, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn quiet() -> FakeWarehouse {
    FakeWarehouse {
        said: Ok("3 models".into()),
    }
}

#[test]
fn warehouse_tools_exist_only_when_the_warehouse_is_configured_and_all_ask() {
    with_server(false, &no_omni, |s| {
        let r = ask(s, "tools/list", json!({}));
        let tools = r["result"]["tools"].as_array().unwrap();
        assert!(
            tools
                .iter()
                .all(|t| !t["name"].as_str().unwrap().contains("warehouse"))
        );
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_warehouse_models", "arguments": {} }),
        );
        assert_eq!(r["error"]["code"], -32602, "{r}");
        let init = ask(s, "initialize", json!({}));
        assert!(
            !init["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("data.warehouse")
        );
    });
    with_warehouse("list", &quiet(), &warehouse_rows, false, |s, _| {
        let r = ask(s, "tools/list", json!({}));
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "tiles_chart",
                "tiles_theme",
                "tiles_warehouse_models",
                "tiles_warehouse_columns"
            ]
        );
        for tool in tools.iter().filter(|t| t["name"] != "tiles_theme") {
            assert_eq!(tool["annotations"]["readOnlyHint"], false, "{tool}");
            assert_eq!(tool["annotations"]["openWorldHint"], true, "{tool}");
        }
        assert!(
            tools[0]["description"]
                .as_str()
                .unwrap()
                .contains("pass data.warehouse {sql, target?, limit?}")
        );
        let init = ask(s, "initialize", json!({}));
        assert!(
            init["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("Pass data.warehouse with a SELECT.")
        );
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_warehouse_models", "arguments": { "target": "dev" } }),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(text_of(&r), "3 models");
        let r = ask(
            s,
            "tools/call",
            json!({ "name": "tiles_warehouse_models", "arguments": "dev" }),
        );
        assert_eq!(r["error"]["code"], -32602, "{r}");
    });
}

#[test]
fn a_warehouse_card_keeps_its_values_and_rows_on_this_machine() {
    with_warehouse("values", &quiet(), &warehouse_rows, false, |s, dir| {
        let r = call(s, serde_json::from_str(WAREHOUSE_INTENT).unwrap());
        assert_eq!(r["result"]["isError"], false, "{r}");
        let text = text_of(&r);
        assert!(
            !text.contains(PLANTED_ROW) && !text.contains("4217"),
            "{text}"
        );
        assert!(
            text.contains(
                "The rows came from BigQuery, so the values on the card stay on this machine"
            ),
            "{text}"
        );
        assert!(
            text.contains("Read fct_revenue from the dev build."),
            "{text}"
        );
        let id = id_of(&text);
        let saved = std::fs::read_to_string(dir.join(format!("cards/{id}.call.json"))).unwrap();
        assert!(
            !saved.contains(PLANTED_ROW),
            "no row in the call file: {saved}"
        );
        let saved: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved["fetched"], json!(["BigQuery"]));
        assert_eq!(
            saved["labels"],
            json!(["dev build · personal_ada_lovelace.fct_revenue"])
        );
        assert_eq!(saved["omni"], json!([false]));
        for entry in std::fs::read_dir(dir.join("cards")).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains(PLANTED_ROW),
                "no row reaches the disk"
            );
        }
    });
    with_warehouse("shared", &quiet(), &warehouse_rows, true, |s, _| {
        let r = call(s, serde_json::from_str(WAREHOUSE_INTENT).unwrap());
        assert!(text_of(&r).contains(PLANTED_ROW), "the explicit opt-in");
    });
    let refuse = |_: Query<'_>, _: Instant| -> Result<Fetched, String> {
        Err("BigQuery refused the dry run: Unrecognized name: regoin".into())
    };
    with_warehouse("refused", &quiet(), &refuse, false, |s, _| {
        let r = call(s, serde_json::from_str(WAREHOUSE_INTENT).unwrap());
        assert_eq!(r["result"]["isError"], true);
        assert!(
            text_of(&r).contains("tiles_chart refused: warehouse: BigQuery refused the dry run"),
            "{r}"
        );
    });
}

#[test]
fn the_hostile_corpus_never_reaches_a_warehouse_result_raw() {
    let plain = |r: &Value| {
        !r.to_string().contains("\\u001b")
            && text_of(r)
                .lines()
                .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char))
    };
    for e in corpus() {
        for said in [Ok(format!("model {e}\ncolumn {e}")), Err(e.clone())] {
            let source = FakeWarehouse { said };
            with_warehouse("hostile-tool", &source, &warehouse_rows, false, |s, _| {
                for tool in ["tiles_warehouse_models", "tiles_warehouse_columns"] {
                    let r = ask(
                        s,
                        "tools/call",
                        json!({ "name": tool, "arguments": { "target": e } }),
                    );
                    assert!(plain(&r), "{tool} {e:?}: {r}");
                }
            });
        }
        let noted = |_: Query<'_>, at: Instant| {
            let mut fetched = warehouse_rows(Query::Warehouse(&dummy_source()), at)?;
            fetched.notes = vec![e.clone()];
            fetched.origin = e.clone();
            Ok(fetched)
        };
        with_warehouse("hostile-note", &quiet(), &noted, false, |s, _| {
            let r = call(s, serde_json::from_str(WAREHOUSE_INTENT).unwrap());
            assert!(plain(&r), "note {e:?}: {r}");
        });
        let refused = |_: Query<'_>, _: Instant| -> Result<Fetched, String> { Err(e.clone()) };
        with_warehouse("hostile-refusal", &quiet(), &refused, false, |s, _| {
            let r = call(s, serde_json::from_str(WAREHOUSE_INTENT).unwrap());
            assert!(plain(&r), "refusal {e:?}: {r}");
        });
    }
}

fn dummy_source() -> tiles_intent::source::WarehouseSource {
    tiles_intent::source::WarehouseSource {
        sql: "SELECT 1".into(),
        target: None,
        limit: None,
    }
}

/// A theme the build carries other than its default, so switching to it
/// redraws.
fn other_theme() -> &'static str {
    crate::card::Theme::all()
        .iter()
        .find(|t| **t != crate::card::Theme::default())
        .expect("the build carries more than one theme")
        .name()
}

/// What the fake painter draws in the theme named `name`.
fn fake_in(name: &str) -> Vec<u8> {
    [b"\x89PNG fake ".as_slice(), name.as_bytes()].concat()
}

/// What the fake painter draws in the default theme.
fn fake_default() -> Vec<u8> {
    fake_in(crate::card::Theme::default().name())
}
