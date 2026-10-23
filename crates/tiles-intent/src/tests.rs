use super::{
    Cell, ClaimKind, IntentError, MAX_PAYLOAD_BYTES, MAX_ROWS, Tone, Unit, parse, parse_request,
};

const GOOD: &str = r#"{
  "v": 1,
  "form": "column",
  "title": "Signups dipped in W3",
  "claim": { "kind": "change", "tone": "bad" },
  "fields": { "x": "week", "y": "signups" },
  "highlight": ["W3"],
  "data": { "rows": [
    { "week": "W1", "signups": 412 },
    { "week": "W2", "signups": 431 },
    { "week": "W3", "signups": 298 }
  ] }
}"#;

fn err(json: &str) -> IntentError {
    parse(json).expect_err("the intent should be refused")
}

fn with(key: &str, value: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
    v[key] = serde_json::from_str(value).unwrap();
    v.to_string()
}

#[test]
fn parses_the_adr_example() {
    let intent = parse(GOOD).unwrap();
    assert_eq!(intent.form, "column");
    assert_eq!(intent.claim.kind, ClaimKind::Change);
    assert_eq!(intent.claim.tone, Tone::Bad);
    assert_eq!(intent.highlight, vec!["W3"]);
    assert_eq!(intent.table.rows.len(), 3);
    let y = intent.column("y").unwrap();
    assert_eq!(intent.table.rows[2][y], Cell::Number(298.0));
    assert!(
        intent
            .require_category_values(&[intent.column("x").unwrap()])
            .is_ok()
    );
}

#[test]
fn unknown_keys_are_left_out_and_noted() {
    let i = parse(&with("colour", "\"red\"")).unwrap();
    assert_eq!(
        i.notes,
        ["Tiles left out the keys it does not take: colour."]
    );
    assert_eq!(parse(GOOD).unwrap().notes, Vec::<String>::new());
}

#[test]
fn unnest_lifts_only_a_lone_intent_object() {
    let flat: serde_json::Value = serde_json::from_str(GOOD).unwrap();
    let mut v = serde_json::json!({ "intent": flat.clone() });
    assert!(super::unnest(&mut v));
    assert_eq!(v, flat);
    // Beside another key, or holding no object, intent stays as sent.
    for sent in [
        serde_json::json!({ "intent": flat.clone(), "v": 1 }),
        serde_json::json!({ "intent": "a chart" }),
        flat.clone(),
    ] {
        let mut v = sent.clone();
        assert!(!super::unnest(&mut v));
        assert_eq!(v, sent);
    }
    // parse_request does not lift: intent is a key it does not take.
    let e = err(&format!(r#"{{"intent":{GOOD}}}"#));
    assert_eq!(e.key, "arguments");
    assert!(
        e.problem
            .ends_with("and Tiles left out the keys it does not take: intent"),
        "{e}"
    );
}

#[test]
fn a_refusal_names_the_keys_tiles_left_out() {
    let e = err(&format!(r#"{{"chart":{GOOD}}}"#));
    assert_eq!(e.key, "arguments");
    assert!(
        e.problem.starts_with("not a valid chart: missing field")
            && e.problem
                .ends_with("and Tiles left out the keys it does not take: chart"),
        "{e}"
    );
    assert!(e.fix.contains("not nested under another key"), "{e}");
    let e = err(&with("colour", "\"red\"").replace("\"form\"", "\"chart_form\""));
    assert!(
        e.problem.contains("chart_form") && e.problem.contains("colour"),
        "{e}"
    );
}

#[test]
fn any_version_reads_as_1() {
    let i = parse(&with("v", "2")).unwrap();
    assert_eq!(i.notes, ["Tiles read v as 1."]);
    let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
    v.as_object_mut().unwrap().remove("v");
    assert_eq!(parse(&v.to_string()).unwrap().notes, Vec::<String>::new());
}

#[test]
fn a_missing_or_unknown_claim_takes_the_default() {
    let i = parse(&with("claim", r#"{"kind":"growth","tone":"Good"}"#)).unwrap();
    assert_eq!(
        (i.claim.kind, i.claim.tone),
        (ClaimKind::Compare, Tone::Good)
    );
    assert_eq!(
        i.notes,
        [
            "Tiles read claim.kind as compare.",
            "Tiles read claim.tone as good."
        ]
    );
    let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
    v.as_object_mut().unwrap().remove("claim");
    let i = parse(&v.to_string()).unwrap();
    assert_eq!(i.claim.tone, Tone::Neutral);
    assert_eq!(i.notes.len(), 2);
}

#[test]
fn a_field_naming_a_missing_column_lists_the_columns() {
    let e = err(&with("fields", r#"{"x":"week","y":"sales"}"#));
    assert_eq!(e.key, "fields.y");
    assert!(e.fix.contains("signups") && e.fix.contains("week"), "{e}");
}

#[test]
fn a_highlight_not_in_the_data_is_refused() {
    let intent = parse(&with("highlight", r#"["W9"]"#)).unwrap();
    let e = intent
        .require_category_values(&[intent.column("x").unwrap()])
        .unwrap_err();
    assert_eq!(e.key, "highlight[0]");
}

#[test]
fn numeric_highlights_match_whole_number_categories() {
    let json = r#"{"v":1,"form":"column","title":"2024 led","claim":{"kind":"rank","tone":"good"},
      "fields":{"x":"year","y":"n"},"highlight":[2024],
      "data":{"rows":[{"year":2023,"n":1},{"year":2024,"n":2}]}}"#;
    let intent = parse(json).unwrap();
    assert!(
        intent
            .require_category_values(&[intent.column("x").unwrap()])
            .is_ok()
    );
}

#[test]
fn control_characters_are_refused_everywhere() {
    for (key, value, what) in [
        ("title", "\"Sales \\u001b]52;c;aGk=\\u0007 up\"", "control"),
        ("subtitle", "\"a\\u202Eb\"", "invisible"),
        ("form", "\"col\\u0000umn\"", "control"),
    ] {
        let e = err(&with(key, value));
        assert_eq!(e.key, key, "{e}");
        assert!(e.problem.contains(&format!("{what} character")), "{e}");
    }
    let e = err(&with(
        "data",
        r#"{"rows":[{"week":"W1\u009b2J","signups":1}]}"#,
    ));
    assert_eq!(e.key, "data.rows[0].week");
}

#[test]
fn zero_width_and_format_characters_are_refused_as_invisible() {
    for hidden in ["\u{200B}", "\u{FEFF}", "\u{00AD}", "\u{2060}"] {
        let e = err(&GOOD.replace("Signups dipped", &format!("Signups{hidden} dipped")));
        assert_eq!(e.key, "title");
        assert!(
            e.problem.starts_with("contains the invisible character U+"),
            "{e}"
        );
    }
}

#[test]
fn invisible_tag_characters_are_refused_in_rows() {
    // U+E0049 U+E0047: tag letters that render as nothing but read as "IG".
    let e = err(&with(
        "data",
        r#"{"rows":[{"week":"Acme\uDB40\uDC49\uDB40\uDC47","signups":1}]}"#,
    ));
    assert_eq!(e.key, "data.rows[0].week");
}

#[test]
fn error_text_never_echoes_control_characters() {
    // serde quotes the unknown key in its message; the message is cleaned.
    let e = err("{\"v\":1,\"\\u001b[2J\":1}");
    let line = e.to_string();
    assert!(!line.chars().any(super::forbidden), "{line:?}");
}

#[test]
fn oversized_and_overlong_input_is_refused() {
    let big = format!("{{\"pad\":\"{}\"}}", "a".repeat(MAX_PAYLOAD_BYTES));
    assert!(err(&big).problem.contains("byte limit"));
    // A long title is cut at a word with an ellipsis, not refused.
    let long = format!(
        "\"Signups dipped in W3 {}\"",
        "after the checkout outage ".repeat(4)
    );
    let i = parse(&with("title", &long)).unwrap();
    assert!(i.title.chars().count() <= 90, "{}", i.title);
    assert!(i.title.ends_with(" the…"), "cut at a word: {}", i.title);
    assert_eq!(i.notes, ["Tiles cut title to 90 characters."]);
    let rows: Vec<String> = (0..=MAX_ROWS)
        .map(|i| format!("{{\"week\":\"W{i}\",\"signups\":{i}}}"))
        .collect();
    let e = err(&with("data", &format!("{{\"rows\":[{}]}}", rows.join(","))));
    assert_eq!(e.key, "data.rows");
}

#[test]
fn deep_nesting_is_refused_without_overflowing_the_stack() {
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let e = err(&format!(r#"{{"v":1,"form":{deep}}}"#));
    assert_eq!(e.key, "arguments");
}

#[test]
fn non_finite_and_nested_values_are_refused() {
    let huge = GOOD.replace("412", "1e400");
    let e = err(&huge);
    assert!(e.problem.contains("out of range"), "{e}");
    let e = err(&with("data", r#"{"rows":[{"week":"W1","signups":[1]}]}"#));
    assert_eq!(e.key, "data.rows[0].signups");
    let e = err(&with("data", r#"{"rows":[{"week":"W1","signups":true}]}"#));
    assert_eq!(e.key, "data.rows[0].signups");
}

// The security review's F8: two 1.7e308 stacks summed to infinity and drew
// no bar, and a 1.7e308 waterfall step printed 300 digits across the card.
#[test]
fn numbers_too_large_to_draw_are_refused_with_their_column() {
    let e = err(&GOOD.replace("431", "1.7e308"));
    assert_eq!(e.key, "data.rows[1].signups");
    assert!(e.problem.contains("larger than 1e15"), "{e}");
    assert!(
        parse(&GOOD.replace("431", "-1e15")).is_ok(),
        "the limit itself draws"
    );
    assert!(
        err(&GOOD.replace("431", "-1.000001e15"))
            .problem
            .contains("1e15")
    );
    let table = omni_table(r#"[{"orders.created_at[week]":"W1","orders.count":2e16}]"#);
    let e = parse_request(OMNI).unwrap().with_table(&table).unwrap_err();
    assert_eq!(e.key, "data.omni");
    assert!(e.problem.contains("orders.count"), "{e}");
}

#[test]
fn empty_rows_are_refused_and_a_missing_title_is_the_measure() {
    assert_eq!(err(&with("data", r#"{"rows":[]}"#)).key, "data.rows");
    let i = parse(&with("title", "\"   \"")).unwrap();
    assert_eq!(i.title, "signups");
    assert_eq!(
        i.notes,
        ["The call had no title, so Tiles titled the card \"signups\"."]
    );
}

/// A missing title falls back to the measure's column name when an intent
/// would accept that name as text, an emoji sequence's joiner included.
#[test]
fn a_missing_title_is_a_measure_named_with_an_emoji_sequence() {
    let titled = |measure: &str| {
        let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
        v["title"] = "".into();
        v["fields"]["y"] = measure.into();
        for row in v["data"]["rows"].as_array_mut().unwrap() {
            let n = row.as_object_mut().unwrap().remove("signups").unwrap();
            row[measure] = n;
        }
        parse(&v.to_string()).map(|i| i.title)
    };
    assert_eq!(
        titled("👩🏽\u{200D}💻 signups").unwrap(),
        "👩🏽\u{200D}💻 signups"
    );
    assert_eq!(titled("sign\u{200D}ups").unwrap_err().key, "fields.y");
}

#[test]
fn truncated_and_garbage_input_never_panics() {
    for cut in 0..GOOD.len() {
        if GOOD.is_char_boundary(cut) {
            let _ = parse(&GOOD[..cut]);
        }
    }
    for junk in [
        "",
        "null",
        "[]",
        "1",
        "\"x\"",
        "{}",
        "{\"v\":-1}",
        "\u{feff}{}",
    ] {
        assert!(parse(junk).is_err(), "{junk:?}");
    }
}

#[test]
fn missing_cells_become_null() {
    let intent = parse(&with(
        "data",
        r#"{"rows":[{"week":"W1","signups":1},{"week":"W2"}]}"#,
    ))
    .unwrap();
    assert_eq!(
        intent.table.rows[1][intent.column("y").unwrap()],
        Cell::Null
    );
}

#[test]
fn unit_and_note_are_optional_and_checked() {
    let plain = parse(GOOD).unwrap();
    assert_eq!((plain.unit, plain.note), (None, None));
    let i = parse(&with("unit", "\"percent\"")).unwrap();
    assert_eq!(i.unit, Some(Unit::Percent));
    let i = parse(&with("unit", "\"yen\"")).unwrap();
    assert_eq!(i.unit, None);
    let i = parse(&with("note", "\"  Checkout bug, 4 to 9 Nov \"")).unwrap();
    assert_eq!(i.note.as_deref(), Some("Checkout bug, 4 to 9 Nov"));
    let i = parse(&with("note", &format!("\"{}\"", "x".repeat(49)))).unwrap();
    assert_eq!(i.note.as_deref().map(|n| n.chars().count()), Some(48));
    let e = err(&with("note", "\"bell\\u0007\""));
    assert_eq!(
        e.key, "note",
        "control characters are still refused (ADR-009)"
    );
    let mut v: serde_json::Value = serde_json::from_str(&with("note", "\"Still 52 min\"")).unwrap();
    v.as_object_mut().unwrap().remove("highlight");
    let i = parse(&v.to_string()).unwrap();
    assert_eq!(i.note, None);
    assert_eq!(
        i.notes,
        ["Tiles left the note out, since no highlighted mark carries it."]
    );
}

const OMNI: &str = r#"{
  "v": 1,
  "form": "column",
  "title": "Orders peaked in W2",
  "claim": { "kind": "rank", "tone": "good" },
  "fields": { "x": "orders.created_at[week]", "y": "orders.count" },
  "highlight": ["W2"],
  "data": { "omni": { "topic": "orders", "fields": ["orders.created_at[WEEK]", "orders.count"] } }
}"#;

fn omni_table(rows: &str) -> crate::source::Table {
    let rows: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_str(rows).unwrap();
    let columns = [
        "orders.created_at[week]".to_string(),
        "orders.count".to_string(),
    ];
    crate::source::Table::from_json_rows(rows, Some(&columns), "omni.query").unwrap()
}

#[test]
fn an_omni_intent_needs_its_rows_fetched() {
    let request = parse_request(OMNI).unwrap();
    let omni = request.omni().unwrap();
    assert_eq!(omni.topic, "orders");
    assert_eq!(omni.fields[0], "orders.created_at[week]");
    assert_eq!(err(OMNI).key, "data.omni");
}

#[test]
fn fetched_omni_rows_complete_the_intent() {
    let table = omni_table(
        r#"[{"orders.created_at[week]":"W1","orders.count":3},
            {"orders.created_at[week]":"W2","orders.count":9}]"#,
    );
    let intent = parse_request(OMNI).unwrap().with_table(&table).unwrap();
    assert_eq!(intent.table.rows.len(), 2);
    let y = intent.column("y").unwrap();
    assert_eq!(intent.table.rows[1][y], Cell::Number(9.0));
    assert_eq!(intent.highlight, vec!["W2"]);
}

// A zero width non-joiner spells many Persian and Indic words, and a zero
// width joiner builds emoji such as 👩‍💻; refusing every format character
// refused whole Omni charts over one such cell.
#[test]
fn fetched_text_loses_invisible_characters_and_keeps_emoji_whole() {
    let table = omni_table(
        r#"[{"orders.created_at[week]":"\u0645\u06cc\u200c\u062e\u0648\u0627\u0647\u0645","orders.count":3},
            {"orders.created_at[week]":"\ud83d\udc69\u200d\ud83d\udcbb W2\u00ad\udb40\udc49","orders.count":9}]"#,
    );
    let intent = parse_request(OMNI).unwrap().with_table(&table).unwrap();
    let x = intent.column("x").unwrap();
    assert_eq!(
        intent.table.rows[0][x],
        Cell::Text("\u{645}\u{6cc}\u{62e}\u{648}\u{627}\u{647}\u{645}".into())
    );
    assert_eq!(
        intent.table.rows[1][x],
        Cell::Text("👩\u{200D}💻 W2".into())
    );
    let rows: Vec<serde_json::Map<String, serde_json::Value>> =
        serde_json::from_str(r#"[{"orders.created_at[week]":"W\u2028","orders.count":1}]"#)
            .unwrap();
    let columns = [
        "orders.created_at[week]".to_string(),
        "orders.count".to_string(),
    ];
    let e = crate::source::Table::from_json_rows(rows, Some(&columns), "omni.query").unwrap_err();
    assert!(
        e.problem().contains("an invisible character at position 1"),
        "{e}"
    );
}

#[test]
fn fetched_rows_missing_a_charted_field_or_empty_are_refused() {
    let mut v: serde_json::Value = serde_json::from_str(OMNI).unwrap();
    v["fields"]["y"] = serde_json::json!("orders.total");
    let request = parse_request(&v.to_string()).unwrap();
    let table = omni_table(r#"[{"orders.created_at[week]":"W1","orders.count":3}]"#);
    assert_eq!(request.with_table(&table).unwrap_err().key, "fields.y");
    let empty = omni_table("[]");
    let e = parse_request(OMNI).unwrap().with_table(&empty).unwrap_err();
    assert_eq!(e.key, "data.omni");
}

#[test]
fn a_bad_omni_source_is_refused_before_any_query() {
    let mut v: serde_json::Value = serde_json::from_str(OMNI).unwrap();
    v["data"]["omni"]["topic"] = serde_json::json!("../admin");
    assert_eq!(
        parse_request(&v.to_string()).unwrap_err().key,
        "data.omni.topic"
    );
    v["data"] = serde_json::json!({ "rows": [], "omni": { "topic": "orders", "fields": ["a.b"] } });
    assert_eq!(parse_request(&v.to_string()).unwrap_err().key, "arguments");
}

#[test]
fn line_and_paragraph_separators_are_refused() {
    for sep in ["\u{2028}", "\u{2029}"] {
        let e = err(&with(
            "title",
            &serde_json::to_string(&format!("a{sep}b")).unwrap(),
        ));
        assert_eq!(e.key, "title");
        assert!(e.problem.contains("invisible character"), "{}", e.problem);
    }
}

#[test]
fn rows_scaled_down_from_the_title_are_refused() {
    let card = |title: &str, unit: &str, rows: &[f64]| {
        let rows: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, v)| format!(r#"{{"month":"M{i}","revenue":{v}}}"#))
            .collect();
        format!(
            r#"{{"v":1,"form":"big-number","title":"{title}","claim":{{"kind":"trend","tone":"good"}},
            "fields":{{"x":"month","y":"revenue"}}{unit},
            "data":{{"rows":[{}]}}}}"#,
            rows.join(",")
        )
    };
    let usd = r#","unit":"usd""#;
    let e = err(&card("Monthly revenue rose to $128K", usd, &[120.0, 128.0]));
    assert_eq!(
        e.to_string(),
        "data.rows: values look pre-scaled: the title says 128K but the rows hold it without its step; \
         give full values (128000, not 128 for $128K); Tiles abbreviates them"
    );
    assert!(
        err(&card("Users reached 84K", "", &[79.0, 84.0]))
            .problem
            .contains("84K")
    );
    assert!(
        err(&card("ARR hit €4.8M", usd, &[4.5, 4.8]))
            .problem
            .contains("4.8M")
    );
    // Full values, plain numbers and figures that are not the headline pass.
    let ok = |title: &str, unit: &str, rows: &[f64]| parse(&card(title, unit, rows)).is_ok();
    assert!(ok(
        "Monthly revenue rose to $128K",
        usd,
        &[120_000.0, 128_000.0]
    ));
    assert!(ok("Monthly revenue rose to $128,000", usd, &[120.0, 128.0]));
    assert!(ok("Revenue rose for 3 months", usd, &[120.0, 128.0]));
    assert!(ok("1M users, churn at 0.4", "", &[0.5, 0.4]));
    assert!(ok("Q4K review: revenue at 128", usd, &[120.0, 128.0]));
    assert!(ok(
        "NPS among 50K customers held at 48",
        "",
        &[40.0, 62.0, 55.0, 48.0]
    ));
    assert!(ok(
        "Average order $52 across 50K orders",
        usd,
        &[49.0, 51.0, 52.0]
    ));
    assert!(ok(
        "Churn among 1M users held at 1%",
        r#","unit":"percent""#,
        &[1.2, 0.9, 1.0]
    ));
    assert!(ok(
        "Signups, trailing 12M",
        "",
        &[11_400.0, 11_900.0, 12_000.0]
    ));
}

#[test]
fn a_count_given_the_percent_unit_draws_as_a_count() {
    let card = |unit: &str, oct: f64| {
        format!(
            r#"{{"v":1,"form":"big-number","title":"Monthly active users","claim":{{"kind":"trend","tone":"good"}},
            "fields":{{"x":"month","y":"users"}},"unit":"{unit}",
            "data":{{"rows":[{{"month":"Sep","users":47800}},{{"month":"Oct","users":{oct}}}]}}}}"#
        )
    };
    let i = parse(&card("percent", 48_600.0)).unwrap();
    assert_eq!(i.unit, None);
    assert_eq!(
        i.notes,
        ["The measure reaches 1,000 or more, so Tiles drew it as a count, not a percentage."]
    );
    assert!(
        !i.notes[0].contains("48"),
        "no row value in the note (ADR-013)"
    );
    assert!(parse(&card("usd", 48_600.0)).is_ok());
    // Shares and growth rates in points pass, up to 999%.
    let share = r#"{"v":1,"form":"big-number","title":"Share","claim":{"kind":"trend","tone":"good"},
        "fields":{"x":"m","y":"s"},"unit":"percent","data":{"rows":[{"m":"Sep","s":31},{"m":"Oct","s":999}]}}"#;
    assert!(parse(share).is_ok());
}

#[test]
fn a_percent_measure_given_as_fractions_is_refused() {
    let card = |title: &str, y: &str, rows: &[f64]| {
        let rows: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, v)| format!(r#"{{"week":"W{i}","{y}":{v}}}"#))
            .collect();
        format!(
            r#"{{"v":1,"form":"line","title":"{title}","claim":{{"kind":"trend","tone":"bad"}},
            "fields":{{"x":"week","y":"{y}"}},"unit":"percent","data":{{"rows":[{}]}}}}"#,
            rows.join(",")
        )
    };
    let e = err(&card(
        "Checkout conversion fell to 3.2%",
        "conversion",
        &[0.038, 0.035, 0.032],
    ));
    assert_eq!(
        e.to_string(),
        "data.rows: every \"conversion\" value lies between -1 and 1, so the rows look like \
         fractions of 1 where the percent unit takes points; resend the values in points \
         (4.1, not 0.041); if they are points under 1, say so in the title (0.4%, or 0.5 points)"
    );
    for v in ["0.038", "0.035", "0.032"] {
        assert!(!e.to_string().contains(v), "no row value (ADR-013)");
    }
    // A rate or share named without a figure reads as percent too.
    assert_eq!(
        err(&card(
            "Conversion by week",
            "conversion_rate",
            &[0.38, 0.41]
        ))
        .key,
        "data.rows"
    );
    assert_eq!(
        err(&card("Share of mobile orders", "mobile", &[0.6, 0.62])).key,
        "data.rows"
    );
    // Points, small percents the title writes, and measures that do not
    // read as percent pass as given.
    let ok = |title: &str, y: &str, rows: &[f64]| {
        let i = parse(&card(title, y, rows)).unwrap();
        assert_eq!(i.unit, Some(Unit::Percent), "{title}");
        let c = i.column("y").unwrap();
        let got: Vec<f64> = i
            .table
            .rows
            .iter()
            .filter_map(|r| r[c].plain_number())
            .collect();
        assert_eq!(got, rows, "Tiles never multiplies");
    };
    ok(
        "Refund rate held under 0.5%",
        "refund_rate",
        &[0.3, 0.4, 0.45],
    );
    ok("Error rate stayed at 1%", "errors", &[0.8, 0.9, 1.0]);
    ok(
        "Conversion rate climbed to 41%",
        "conversion_rate",
        &[0.0, 38.0, 41.0],
    );
    ok(
        "Completion share reached 100%",
        "share",
        &[0.0, 64.0, 100.0],
    );
    ok(
        "Conversion fell 0.5 points in W3",
        "change",
        &[0.2, -0.1, -0.5],
    );
    ok(
        "Conversion rate fell 0.4pp in W3",
        "conversion_rate",
        &[-0.4, -0.1, 0.0],
    );
    ok("Churn rate fell 30bps", "churn_rate", &[0.5, 0.2]);
    ok("Share rose 2pts by week", "share", &[0.1, 0.3]);
    ok("Conversion change by week", "change_pp", &[0.3, -0.5]);
    ok("Uptime held", "uptime", &[0.2, 0.3]);
    ok("Conversion rate by week", "conversion_rate", &[0.0, 0.0]);
}

#[test]
fn a_dumbbell_given_percent_fractions_is_refused() {
    let card = |title: &str, rows: &[(f64, f64)]| {
        let rows: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, (lo, hi))| format!(r#"{{"region":"R{i}","before":{lo},"after":{hi}}}"#))
            .collect();
        format!(
            r#"{{"v":1,"form":"dumbbell","title":"{title}","claim":{{"kind":"change","tone":"good"}},
            "fields":{{"x":"region","lo":"before","hi":"after"}},"unit":"percent","data":{{"rows":[{}]}}}}"#,
            rows.join(",")
        )
    };
    let e = err(&card(
        "Conversion rate rose from 3.1% to 3.9% by region",
        &[(0.031, 0.039), (0.028, 0.035)],
    ));
    assert_eq!(e.key, "data.rows");
    assert!(
        e.to_string()
            .starts_with("data.rows: every \"before\" and \"after\" value lies between")
    );
    for v in ["0.031", "0.039", "0.028", "0.035"] {
        assert!(!e.to_string().contains(v), "no row value (ADR-013)");
    }
    // Each measure is checked on its own: points in one leave the other refused.
    let e = err(&card(
        "Conversion rate rose from 3.1% to 3.9% by region",
        &[(0.031, 3.9), (0.028, 3.5)],
    ));
    assert!(
        e.to_string()
            .starts_with("data.rows: every \"before\" value lies between")
    );
    let i = parse(&card(
        "Conversion rate rose from 3.1% to 3.9% by region",
        &[(3.1, 3.9), (2.8, 3.5)],
    ))
    .unwrap();
    assert_eq!(i.unit, Some(Unit::Percent));
    assert!(
        parse(&card(
            "Conversion rose 0.5 points by region",
            &[(0.2, 0.7), (0.1, 0.6)]
        ))
        .is_ok()
    );
}

#[test]
fn an_empty_unit_means_none() {
    let card = |unit: &str| {
        format!(
            r#"{{"v":1,"form":"big-number","title":"Monthly active users","claim":{{"kind":"trend","tone":"good"}},
            "fields":{{"x":"month","y":"users"}},"unit":{unit},
            "data":{{"rows":[{{"month":"Sep","users":47800}},{{"month":"Oct","users":48600}}]}}}}"#
        )
    };
    let i = parse(&card("\"\"")).unwrap();
    assert_eq!(i.unit, None);
    assert_eq!(i.notes, ["Tiles read unit \"\" as no unit."]);
    assert_eq!(parse(&card("null")).unwrap().unit, None);
    assert_eq!(
        parse(&card("\"usd\"")).unwrap().unit,
        Some(crate::Unit::Usd)
    );
    let i = parse(&card("\"users\"")).unwrap();
    assert_eq!(i.unit, None);
    assert!(i.notes[0].contains("\"users\" is not one it writes"));
    let i = parse(&card("\"$\"")).unwrap();
    assert_eq!(i.unit, Some(crate::Unit::Usd));
    assert_eq!(i.notes, ["Tiles read unit \"$\" as usd."]);
    assert_eq!(parse(&card("\"usd\"")).unwrap().notes, Vec::<String>::new());
    assert_eq!(
        parse(&card("\"count\"")).unwrap().notes,
        ["Tiles read unit \"count\" as no unit."]
    );
    assert_eq!(
        parse(&card("\"%\"")).unwrap().unit,
        None,
        "users reach 1,000"
    );
}

const WAREHOUSE: &str = r#"{
  "v": 1,
  "form": "column",
  "title": "Paid volume rose in October",
  "claim": { "kind": "change", "tone": "good" },
  "fields": { "x": "month", "y": "amount" },
  "data": { "warehouse": {
    "sql": "WITH p AS (SELECT organization_id, DATE_TRUNC(DATE(created_at), MONTH) AS month, amount_eur FROM {{ ref('fct_payments') }}),\n o AS (SELECT * FROM {{- ref(\"acme\", \"stg_organizations\", v=2) -}})\nSELECT month, SUM(amount_eur) AS amount FROM p JOIN o USING (organization_id) JOIN `acme-prod.dbt_staging.stg_support_tickets` USING (organization_id) GROUP BY 1",
    "target": "dev",
    "limit": 500
  } }
}"#;

/// The warehouse intent with its source's key set to `value`.
fn warehouse_with(key: &str, value: serde_json::Value) -> String {
    let mut v: serde_json::Value = serde_json::from_str(WAREHOUSE).unwrap();
    v["data"]["warehouse"][key] = value;
    v.to_string()
}

fn warehouse_err(key: &str, value: serde_json::Value) -> IntentError {
    parse_request(&warehouse_with(key, value)).expect_err("the source should be refused")
}

#[test]
fn a_warehouse_query_reads_its_model_references() {
    let request = parse_request(WAREHOUSE).unwrap();
    let source = request.warehouse().unwrap();
    assert_eq!(source.target.as_deref(), Some("dev"));
    assert_eq!(source.effective_limit(), 500);
    let references = source.references().unwrap();
    let names: Vec<String> = references
        .iter()
        .map(crate::source::ModelReference::model_name)
        .collect();
    assert_eq!(names, ["fct_payments", "model.acme.stg_organizations.v2"]);
    assert_eq!(
        &source.sql[references[0].span.clone()],
        "{{ ref('fct_payments') }}"
    );
    let rendered = source
        .render(|r| {
            Ok::<_, crate::source::DataError>(format!(
                "`acme-dev.personal_ada_lovelace.{}`",
                r.name
            ))
        })
        .unwrap();
    assert!(rendered.contains("FROM `acme-dev.personal_ada_lovelace.fct_payments`),"));
    assert!(rendered.contains("FROM `acme-dev.personal_ada_lovelace.stg_organizations`)"));
    assert!(!rendered.contains("{{"));
    assert_eq!(err(WAREHOUSE).key, "data.warehouse");
    assert!(request.omni().is_none());
}

#[test]
fn a_query_without_models_takes_no_target_and_the_default_limit() {
    let mut v: serde_json::Value = serde_json::from_str(WAREHOUSE).unwrap();
    v["data"] = serde_json::json!({ "warehouse": {
        "sql": "SELECT month, amount FROM `acme-prod.dbt_marts.fct_monthly`"
    }});
    let request = parse_request(&v.to_string()).unwrap();
    let source = request.warehouse().unwrap();
    assert_eq!(source.effective_limit(), 5_000);
    assert_eq!(source.references().unwrap().len(), 0);
    v["data"]["warehouse"]["target"] = serde_json::json!("prod");
    let e = parse_request(&v.to_string()).unwrap_err();
    assert_eq!(e.key, "data.warehouse.target");
    assert!(e.problem.contains("no {{ ref('model') }}"), "{e}");
}

#[test]
fn a_query_reading_models_needs_a_known_target() {
    let mut v: serde_json::Value = serde_json::from_str(WAREHOUSE).unwrap();
    v["data"]["warehouse"]
        .as_object_mut()
        .unwrap()
        .remove("target");
    let e = parse_request(&v.to_string()).unwrap_err();
    assert_eq!(e.key, "data.warehouse.target");
    assert!(e.fix.contains("ask them"), "{e}");
    for target in ["mr:4521", "mr:4521:full", "prod", "mr:999999999"] {
        assert!(
            parse_request(&warehouse_with("target", serde_json::json!(target))).is_ok(),
            "{target}"
        );
    }
    for target in [
        "Prod",
        "staging",
        "mr:",
        "mr:0",
        "mr:-1",
        "mr:1000000000",
        "mr:12:partial",
        "mr:12:full:full",
        "mr:\u{1b}[2J",
    ] {
        let e = warehouse_err("target", serde_json::json!(target));
        assert_eq!(e.key, "data.warehouse.target", "{target}");
        assert!(!e.to_string().contains('\u{1b}'));
    }
}

#[test]
fn every_dbt_spelling_of_a_reference_is_read() {
    for (sql, name) in [
        ("SELECT * FROM {{ref('fct_x')}}", "fct_x"),
        ("SELECT * FROM {{   ref( \"fct_x\" )   }}", "fct_x"),
        ("SELECT * FROM {{- ref('fct_x') -}}", "fct_x"),
        ("SELECT * FROM {{ ref('fct_x', v=2) }}", "fct_x.v2"),
        ("SELECT * FROM {{ ref('fct_x', version='3') }}", "fct_x.v3"),
        ("SELECT * FROM {{ ref('pkg', 'fct-x') }}", "model.pkg.fct-x"),
        (
            "SELECT * FROM {{ ref('pkg', 'fct_x', v=1,) }}",
            "model.pkg.fct_x.v1",
        ),
        // dbt renders a placeholder in a comment too.
        ("SELECT 1 -- {{ ref('fct_x') }}", "fct_x"),
    ] {
        let source = crate::source::WarehouseSource {
            sql: sql.into(),
            target: Some("prod".into()),
            limit: None,
        };
        assert_eq!(source.validate(), Ok(()), "{sql}");
        let references = source.references().unwrap();
        assert_eq!(references.len(), 1, "{sql}");
        assert_eq!(references[0].model_name(), name, "{sql}");
    }
}

#[test]
fn a_placeholder_tiles_cannot_read_is_refused_without_quoting_it() {
    for (sql, problem) in [
        (
            "SELECT 1 FROM {{ ref('a') }} JOIN {{ source('crm', 'users') }}",
            "source(...)",
        ),
        (
            "SELECT {{ config(materialized='table') }} 1",
            "not a {{ ref('model') }}",
        ),
        ("SELECT {{ var('x') }}", "not a {{ ref('model') }}"),
        ("SELECT * FROM {{ ref('a')", "without closing it"),
        ("SELECT * FROM {{ ref() }}", "one or two quoted names"),
        (
            "SELECT * FROM {{ ref('a', 'b', 'c') }}",
            "one or two quoted names",
        ),
        ("SELECT * FROM {{ ref('a b') }}", "letters, digits, _ and -"),
        (
            "SELECT * FROM {{ ref('a;DROP') }}",
            "letters, digits, _ and -",
        ),
        ("SELECT * FROM {{ ref('a\\'b') }}", "backslash"),
        ("SELECT * FROM {{ ref('a) }}", "never closes"),
        ("SELECT * FROM {{ ref('a' 'b') }}", "separated by commas"),
        ("SELECT * FROM {{ ref('a', v=2, v=3) }}", "two versions"),
        (
            "SELECT * FROM {{ ref('a', v=2, 'b') }}",
            "after its version",
        ),
        ("SELECT * FROM {{ ref('a', alias='b') }}", "v= or version="),
        ("SELECT * FROM {{ ref('a', v='2.1') }}", "version is not"),
        ("SELECT * FROM {{ ref('pkg-x', 'a') }}", "package is not"),
        ("SELECT * FROM {{ ref('a') | upper }}", "text after"),
    ] {
        let e = crate::source::WarehouseSource {
            sql: sql.into(),
            target: Some("dev".into()),
            limit: None,
        }
        .validate()
        .expect_err(sql);
        assert_eq!(e.key(), "data.warehouse.sql", "{sql}");
        assert!(e.problem().contains(problem), "{sql}: {e}");
        assert!(
            e.problem().contains("at character 14") || !sql.starts_with("SELECT * FROM {{"),
            "{e}"
        );
        assert!(!e.to_string().contains('\u{1b}'));
        assert!(!e.to_string().contains("users'"), "{e}");
    }
    let many = format!("SELECT * FROM {}", "{{ ref('a') }} ".repeat(65));
    let e = crate::source::WarehouseSource {
        sql: many,
        target: Some("dev".into()),
        limit: None,
    }
    .validate()
    .expect_err("too many");
    assert!(e.problem().contains("more than 64"), "{e}");
}

#[test]
fn warehouse_sql_must_be_plain_text_of_bounded_size() {
    for sql in [
        String::new(),
        "  \n\t ".into(),
        "SELECT 1 /* \u{202e} */".into(),
        "SELECT '\u{2066}' AS x".into(),
        "SELECT 1\u{1b}[2J".into(),
        "SELECT 1\u{0}".into(),
        format!("SELECT '{}'", "x".repeat(crate::source::MAX_SQL_BYTES)),
    ] {
        let e = warehouse_err("sql", serde_json::json!(sql));
        assert_eq!(e.key, "data.warehouse.sql");
        assert!(!e.to_string().contains('\u{1b}'));
    }
    assert!(
        parse_request(&warehouse_with(
            "sql",
            serde_json::json!("SELECT\r\n\t1 AS n -- é 日本語 👩‍💻")
        ))
        .is_err(),
        "a target with no model is refused"
    );
    let e = warehouse_err("limit", serde_json::json!(0));
    assert_eq!(e.key, "data.warehouse.limit");
    let typo = warehouse_err("dry_run", serde_json::json!(true));
    assert_eq!(typo.key, "arguments");
}

fn warehouse_table(rows: &str) -> crate::source::Table {
    let rows: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_str(rows).unwrap();
    let columns = ["month".to_string(), "amount".to_string()];
    crate::source::Table::from_json_rows(rows, Some(&columns), "data.warehouse").unwrap()
}

#[test]
fn fetched_warehouse_rows_complete_the_intent_or_name_the_query() {
    let table = warehouse_table(
        r#"[{"month":"2026-09-01","amount":41200.5},{"month":"2026-10-01","amount":48210}]"#,
    );
    let intent = parse_request(WAREHOUSE)
        .unwrap()
        .with_table(&table)
        .unwrap();
    let y = intent.column("y").unwrap();
    assert_eq!(intent.table.rows[1][y], Cell::Number(48210.0));
    let e = parse_request(WAREHOUSE)
        .unwrap()
        .with_table(&warehouse_table("[]"))
        .unwrap_err();
    assert_eq!(e.key, "data.warehouse");
    assert!(e.fix.contains("the query's filters"), "{e}");
    let mut v: serde_json::Value = serde_json::from_str(WAREHOUSE).unwrap();
    v["fields"]["y"] = serde_json::json!("revenue");
    let e = parse_request(&v.to_string())
        .unwrap()
        .with_table(&table)
        .unwrap_err();
    assert_eq!(e.key, "fields.y");
    assert!(e.problem.contains("not in the query's columns"), "{e}");
    let huge = warehouse_table(r#"[{"month":"2026-09-01","amount":2e15}]"#);
    let e = parse_request(WAREHOUSE)
        .unwrap()
        .with_table(&huge)
        .unwrap_err();
    assert_eq!(e.key, "data.warehouse");
}

/// The hostile-text corpus (ADR-009) as a title and a label: an intent
/// refuses every entry but the emoji sequence joined with U+200D.
#[test]
fn the_hostile_corpus_is_refused_but_for_an_emoji_sequence() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut accepted = Vec::new();
    for e in json["entries"].as_array().unwrap() {
        let e = e.as_str().unwrap();
        let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
        v["title"] = e.into();
        let title = parse(&v.to_string());
        let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
        v["data"]["rows"][2]["week"] = e.into();
        v["highlight"] = serde_json::json!([e]);
        let label = parse(&v.to_string());
        assert_eq!(title.is_ok(), label.is_ok(), "{e:?}");
        if title.is_ok() {
            accepted.push(e.to_string());
        }
    }
    assert_eq!(accepted, ["👩🏽\u{200D}💻 Priya"]);
    let e = err(&with("title", r#""Pri‍ya""#));
    assert!(
        e.to_string()
            .contains("contains the invisible character U+200D"),
        "{e}"
    );
}
