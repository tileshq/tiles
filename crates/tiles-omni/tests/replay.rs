//! The Omni client against the loopback replay server and the recorded
//! synthetic fixtures (ADR-008). No test here can reach a real Omni instance:
//! the client only talks to the replay server, which refuses unrecorded calls.

use std::time::Duration;

use serde_json::json;
use tiles_intent::source::{Cell, ColumnKind, DataSource, OmniSource, Sort};
use tiles_omni::replay::{Exchange, ReplayServer, UNRECORDED, recorded};
use tiles_omni::{Endpoint, FieldKind, OmniClient, OmniError, TransportKind};

const KEY: &str = "omni_planted_key_7Yq2xL9vR4";
const MODEL: &str = "4f8c2a1e-0b7d-4c55-9a3e-2d1f6b8e9c01";
const MODELS: &str = "/v1/models?modelKind=SHARED&pageSize=100";

fn server(extra: Vec<Exchange>) -> ReplayServer {
    // Extra exchanges come first so they win over the recorded ones.
    ReplayServer::start(KEY, extra.into_iter().chain(recorded()).collect())
}

fn client(server: &ReplayServer) -> OmniClient {
    OmniClient::new(server.config())
}

fn revenue_by_region() -> OmniSource {
    OmniSource {
        topic: "orders".into(),
        fields: vec!["orders.region".into(), "orders.revenue".into()],
        sorts: vec![Sort {
            field: "orders.revenue".into(),
            desc: true,
        }],
        limit: Some(5),
    }
}

fn query_body(fields: &[&str], sorts: &serde_json::Value, limit: u32) -> serde_json::Value {
    json!({
        "query": {
            "modelId": MODEL,
            "table": "orders",
            "join_paths_from_topic_name": "orders",
            "fields": fields,
            "sorts": sorts,
            "limit": limit,
        },
        "resultType": "json",
        "formatResults": false,
    })
}

/// A recorded query answering `fields` with `body`.
fn rows_reply(fields: &[&str], body: &str) -> Exchange {
    Exchange::new("POST", "/v1/query/run", 200, body.to_owned()).expecting(query_body(
        fields,
        &json!([]),
        5_000,
    ))
}

fn source(fields: &[&str]) -> OmniSource {
    OmniSource {
        topic: "orders".into(),
        fields: fields.iter().map(|f| (*f).to_owned()).collect(),
        sorts: vec![],
        limit: None,
    }
}

#[test]
fn topics_lists_visible_topics_of_shared_models() {
    let server = server(vec![]);
    let listing = client(&server).topics().expect("topics");
    let names: Vec<&str> = listing.topics.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["orders", "subscriptions"]);
    assert_eq!(listing.topics[0].model.as_str(), MODEL);
    assert_eq!(listing.topics[0].label.as_deref(), Some("Orders"));
    assert_eq!(listing.skipped, 0);
    assert_eq!(server.unrecorded(), []);
    assert!(server.seen().iter().all(|s| s.authorized));
}

#[test]
fn fields_lists_dimensions_then_measures_with_timeframes() {
    let server = server(vec![]);
    let topic = client(&server).fields("orders").expect("fields");
    let names: Vec<&str> = topic.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "orders.created_at",
            "orders.region",
            "orders.channel",
            "customers.segment",
            "orders.count",
            "orders.revenue"
        ]
    );
    assert_eq!(
        topic.fields[0].time_frames,
        ["date", "week", "month", "quarter", "year"]
    );
    assert_eq!(topic.fields[5].kind, FieldKind::Measure);
    assert_eq!(topic.fields[5].aggregate.as_deref(), Some("SUM"));
    assert_eq!(topic.base_view.as_deref(), Some("orders"));
    assert!(topic.offers("orders.created_at[week]"));
}

#[test]
fn a_query_returns_typed_rows_in_field_order() {
    let server = server(vec![]);
    let table = client(&server).query(&revenue_by_region()).expect("rows");
    let columns: Vec<(&str, ColumnKind)> = table
        .columns()
        .iter()
        .map(|c| (c.name.as_str(), c.kind))
        .collect();
    assert_eq!(
        columns,
        [
            ("orders.region", ColumnKind::Text),
            ("orders.revenue", ColumnKind::Number)
        ]
    );
    assert_eq!(table.rows().len(), 5);
    assert_eq!(table.rows()[0][0], Cell::Text("Pacific Northwest".into()));
    assert_eq!(table.rows()[0][1], Cell::Number(184_230.5));
    assert_eq!(server.unrecorded(), []);
}

#[test]
fn a_query_by_week_uses_the_timeframe_field() {
    let server = server(vec![]);
    let source = OmniSource {
        topic: "orders".into(),
        fields: vec!["orders.created_at[week]".into(), "orders.count".into()],
        sorts: vec![Sort {
            field: "orders.created_at[week]".into(),
            desc: false,
        }],
        limit: Some(12),
    };
    let table = client(&server).query(&source).expect("rows");
    assert_eq!(table.rows().len(), 4);
    assert_eq!(table.columns()[1].kind, ColumnKind::Number);
}

#[test]
fn an_uppercase_timeframe_queries_and_reads_the_lowercase_field() {
    let server = server(vec![]);
    let data: DataSource = serde_json::from_value(json!({ "omni": {
        "topic": "orders",
        "fields": ["orders.created_at[WEEK]", "orders.count"],
        "sorts": [{ "field": "orders.created_at[WEEK]", "desc": false }],
        "limit": 12
    }}))
    .expect("omni source");
    let DataSource::Omni(source) = data else {
        panic!("not omni");
    };
    let table = client(&server).query(&source).expect("rows");
    assert_eq!(table.columns()[0].name, "orders.created_at[week]");
    assert_eq!(table.rows().len(), 4);
    assert_eq!(server.unrecorded(), []);
}

#[test]
fn the_replay_server_refuses_unrecorded_calls() {
    let server = server(vec![]);
    let mut source = revenue_by_region();
    source.limit = Some(7);
    let error = client(&server).query(&source).expect_err("unrecorded");
    assert!(
        matches!(
            error,
            OmniError::Status {
                status: UNRECORDED,
                ..
            }
        ),
        "{error}"
    );
    let missed = server.unrecorded();
    assert_eq!(missed.len(), 1);
    assert_eq!(missed[0].path, "/v1/query/run");
}

#[test]
fn unknown_topics_and_fields_are_refused_before_any_query() {
    let server = server(vec![]);
    let omni = client(&server);
    let error = omni.fields("returns").expect_err("unknown topic");
    assert!(matches!(error, OmniError::UnknownTopic { .. }), "{error}");
    let error = omni
        .query(&source(&["orders.region", "orders.margin"]))
        .expect_err("unknown field");
    assert_eq!(
        error.to_string(),
        "topic `orders` has no field `orders.margin`; run `tiles omni fields orders`"
    );
    assert!(server.seen().iter().all(|s| s.path != "/v1/query/run"));
}

#[test]
fn a_hostile_topic_name_never_reaches_a_url() {
    let server = server(vec![]);
    let error = client(&server)
        .fields("../../v1/models")
        .expect_err("hostile");
    assert!(matches!(error, OmniError::Data(_)), "{error}");
    assert_eq!(server.seen(), []);
}

#[test]
fn a_wrong_key_names_the_fix() {
    let server = ReplayServer::start("some_other_key", recorded());
    let error = client_with_key(&server).topics().expect_err("401");
    assert!(matches!(error, OmniError::Status { status: 401, .. }));
    assert!(error.to_string().contains("TILES_OMNI_API_KEY"));
    assert!(server.seen().iter().all(|s| !s.authorized));
}

fn client_with_key(server: &ReplayServer) -> OmniClient {
    let mut config = server.config();
    config.key = tiles_omni::ApiKey::new(KEY).expect("key");
    OmniClient::new(config)
}

#[test]
fn gateway_errors_are_retried_a_bounded_number_of_times() {
    let server = server(vec![Exchange::new("GET", MODELS, 503, "{}").once()]);
    client(&server).topics().expect("second attempt");
    assert_eq!(server.seen().iter().filter(|s| s.path == MODELS).count(), 2);

    let busy = || {
        Exchange::new("GET", MODELS, 429, "{}")
            .header("Retry-After", "0")
            .once()
    };
    let server = server_only(vec![busy(), busy(), busy(), busy()]);
    let error = client(&server).topics().expect_err("rate limited");
    assert!(
        matches!(error, OmniError::Status { status: 429, .. }),
        "{error}"
    );
    assert_eq!(server.seen().len(), 3);
}

fn server_only(exchanges: Vec<Exchange>) -> ReplayServer {
    ReplayServer::start(KEY, exchanges)
}

#[test]
fn redirects_are_never_followed() {
    let away =
        Exchange::new("GET", MODELS, 302, "").header("Location", "https://evil.test/api/v1/models");
    let server = server_only(vec![away]);
    let error = client(&server).topics().expect_err("redirect");
    assert_eq!(
        error,
        OmniError::Redirect {
            endpoint: Endpoint::ListModels,
            other_host: true
        }
    );
    assert_eq!(server.seen().len(), 1);

    let same = Exchange::new("GET", MODELS, 307, "").header("Location", "/api/v1/models?page=2");
    let server = server_only(vec![same]);
    let error = client(&server).topics().expect_err("redirect");
    assert_eq!(
        error,
        OmniError::Redirect {
            endpoint: Endpoint::ListModels,
            other_host: false
        }
    );
    assert_eq!(server.seen().len(), 1);
}

#[test]
fn oversized_responses_are_cut_off() {
    let huge = format!(
        r#"{{"pageInfo":{{"hasNextPage":false}},"records":[{{"id":"{MODEL}","name":"{}"}}]}}"#,
        "x".repeat(5 * 1024 * 1024)
    );
    let server = server_only(vec![Exchange::new("GET", MODELS, 200, huge)]);
    let error = client(&server).topics().expect_err("too large");
    assert_eq!(
        error,
        OmniError::Transport {
            endpoint: Endpoint::ListModels,
            kind: TransportKind::TooLarge
        }
    );
}

#[test]
fn wrong_types_are_schema_errors_naming_the_path() {
    let cases = [
        (
            r#"{"pageInfo":{"hasNextPage":false},"records":[{"id":42}]}"#,
            "records[0].id",
        ),
        (
            r#"{"pageInfo":{"hasNextPage":false},"records":[{"id":"../../etc"}]}"#,
            "records[0].id",
        ),
        (
            r#"{"pageInfo":{"hasNextPage":true},"records":[]}"#,
            "pageInfo.nextCursor",
        ),
        (r#"{"records":[]}"#, "."),
        (r"[1,2,3]", "[0]"),
        ("<html>gateway</html>", "."),
    ];
    for (body, path) in cases {
        let server = server_only(vec![Exchange::new("GET", MODELS, 200, body)]);
        match client(&server).topics() {
            Err(OmniError::Schema {
                endpoint: Endpoint::ListModels,
                path: got,
                ..
            }) => {
                assert_eq!(got, path, "{body}");
            }
            other => panic!("{body}: {other:?}"),
        }
    }
}

#[test]
fn hostile_topic_and_field_names_from_omni_are_skipped() {
    let topics = json!({ "topics": [
        { "name": "orders" },
        { "name": "orders\u{1b}]52;c;ZXZpbA==\u{7}" },
        { "name": "../../v1/models" },
        { "name": "x".repeat(300) },
    ]});
    let listed = server(vec![Exchange::new(
        "GET",
        &format!("/v1/models/{MODEL}/topic"),
        200,
        topics.to_string(),
    )]);
    let listing = client(&listed).topics().expect("topics");
    assert_eq!(listing.topics.len(), 1);
    assert_eq!(listing.skipped, 3);

    let topic = json!({ "topic": { "name": "orders", "views": [{ "name": "orders",
        "dimensions": [
            { "fully_qualified_name": "orders.region" },
            { "fully_qualified_name": "orders.\u{202E}noiger" },
            { "fully_qualified_name": "orders.region; DROP TABLE" },
            { "field_name": null }
        ],
        "measures": null
    }]}});
    let described = server(vec![Exchange::new(
        "GET",
        &format!("/v1/models/{MODEL}/topic/orders"),
        200,
        topic.to_string(),
    )]);
    let fields = client(&described).fields("orders").expect("fields");
    assert_eq!(fields.fields.len(), 1);
    assert_eq!(fields.skipped, 3);
}

#[test]
fn hostile_row_values_are_refused_without_echoing_them() {
    let fields = ["orders.region", "orders.count"];
    let escape =
        r#"[{"orders.region":"\u001b]8;;https://evil.test\u001b\\click","orders.count":1}]"#;
    let server = server(vec![rows_reply(&fields, escape)]);
    let error = client(&server)
        .query(&source(&fields))
        .expect_err("hostile");
    let text = error.to_string();
    assert!(text.contains("omni rows[0].orders.region"), "{text}");
    assert!(text.contains("control character"), "{text}");
    assert!(!text.contains("evil.test"), "{text}");
}

#[test]
fn bidi_overrides_in_row_text_are_stripped() {
    let fields = ["orders.region", "orders.count"];
    let body = r#"[{"orders.region":"\u202Eecnalab","orders.count":1}]"#;
    let server = server(vec![rows_reply(&fields, body)]);
    let table = client(&server).query(&source(&fields)).expect("rows");
    assert_eq!(table.rows()[0][0], Cell::Text("ecnalab".into()));
}

#[test]
fn line_breaks_in_row_text_become_spaces() {
    let fields = ["orders.region", "orders.count"];
    let body = r#"[{"orders.region":"North\neast\tcoast","orders.count":1}]"#;
    let server = server(vec![rows_reply(&fields, body)]);
    let table = client(&server).query(&source(&fields)).expect("rows");
    assert_eq!(table.rows()[0][0], Cell::Text("North east coast".into()));
}

#[test]
fn malformed_rows_are_refused() {
    let fields = ["orders.region", "orders.count"];
    let huge = format!(
        r#"[{{"orders.region":"{}","orders.count":1}}]"#,
        "y".repeat(1_001)
    );
    let cases = [
        (
            r#"[{"orders.region":"a","orders.count":"12"},{"orders.region":"b","orders.count":3}]"#
                .to_owned(),
            "a text column",
        ),
        (
            r#"[{"orders.region":"a","orders.count":1,"orders.secret":2}]"#.to_owned(),
            "expected columns",
        ),
        (r#"[{"orders.region":"a"}]"#.to_owned(), "expected columns"),
        (
            r#"[{"orders.region":["a"],"orders.count":1}]"#.to_owned(),
            "list or an object",
        ),
        (r#"{"orders.region":"a"}"#.to_owned(), "wrong type"),
        (
            r#"[{"orders.region":"a","orders.count":1e999}]"#.to_owned(),
            "not valid JSON",
        ),
        (huge, "over 1000"),
    ];
    for (body, expected) in cases {
        let server = server(vec![rows_reply(&fields, &body)]);
        let error = client(&server)
            .query(&source(&fields))
            .expect_err("malformed");
        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
}

#[test]
fn more_rows_than_the_limit_are_refused() {
    let fields = ["orders.region", "orders.count"];
    let rows: Vec<_> = (0..6)
        .map(|i| json!({ "orders.region": format!("r{i}"), "orders.count": i }))
        .collect();
    let mut src = source(&fields);
    src.limit = Some(5);
    let reply = Exchange::new("POST", "/v1/query/run", 200, json!(rows).to_string())
        .expecting(query_body(&fields, &json!([]), 5));
    let server = server(vec![reply]);
    let error = client(&server).query(&src).expect_err("too many");
    assert!(
        error.to_string().contains("more rows than the limit"),
        "{error}"
    );
}

#[test]
fn a_slow_query_waits_for_its_jobs_then_reads_the_result() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let timeout =
        json!({ "detail": "Request timeout", "remaining_job_ids": [job], "timed_out": true });
    let server = server(vec![
        Exchange::new("POST", "/v1/query/run", 408, timeout.to_string()).once(),
        Exchange::new(
            "GET",
            &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
            200,
            format!(
                "{}\n{}\n",
                json!({ "job_id": job, "status": "COMPLETE" }),
                json!({ "remaining_job_ids": [], "timed_out": "false" })
            ),
        ),
    ]);
    let table = client(&server)
        .query(&revenue_by_region())
        .expect("rows after waiting");
    assert_eq!(table.rows().len(), 5);
    let runs = server
        .seen()
        .iter()
        .filter(|s| s.path == "/v1/query/run")
        .count();
    assert_eq!(runs, 2);
    assert_eq!(server.unrecorded(), []);
}

#[test]
fn a_failed_slow_query_reports_omnis_error_type() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let timeout = json!({ "remaining_job_ids": [job], "timed_out": true });
    let server = server(vec![
        Exchange::new("POST", "/v1/query/run", 408, timeout.to_string()).once(),
        Exchange::new(
            "GET",
            &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
            200,
            format!(
                "{}\n{}\n",
                json!({ "job_id": job, "status": "FAILED", "error_type": "SQL\u{1b}[2J" }),
                json!({ "remaining_job_ids": [], "timed_out": "false" })
            ),
        ),
    ]);
    let error = client(&server)
        .query(&revenue_by_region())
        .expect_err("failed");
    assert_eq!(
        error,
        OmniError::QueryFailed {
            kind: "SQL[2J".into()
        }
    );
}

#[test]
fn a_query_still_running_after_its_rerun_is_not_a_deadline() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let timeout = json!({ "remaining_job_ids": [job], "timed_out": true });
    let server = server(vec![
        Exchange::new("POST", "/v1/query/run", 408, timeout.to_string()),
        Exchange::new(
            "GET",
            &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
            200,
            format!(
                "{}\n{}\n",
                json!({ "job_id": job, "status": "COMPLETE" }),
                json!({ "remaining_job_ids": [], "timed_out": "false" })
            ),
        ),
    ]);
    let error = client(&server)
        .query(&revenue_by_region())
        .expect_err("still running");
    assert_eq!(error, OmniError::StillRunning);
    assert!(!error.to_string().contains("60 s budget"), "{error}");
}

#[test]
fn a_query_still_running_after_every_wait_is_not_a_deadline() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let timeout = json!({ "remaining_job_ids": [job], "timed_out": true });
    let server = server(vec![
        Exchange::new("POST", "/v1/query/run", 408, timeout.to_string()).once(),
        Exchange::new(
            "GET",
            &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
            200,
            format!(
                "{}\n",
                json!({ "remaining_job_ids": [job], "timed_out": "true" })
            ),
        ),
    ]);
    let error = client(&server)
        .query(&revenue_by_region())
        .expect_err("still running");
    assert_eq!(error, OmniError::StillRunning);
    let waits = server
        .seen()
        .iter()
        .filter(|s| s.path.starts_with("/v1/query/wait"))
        .count();
    assert_eq!(waits, 10);
}

#[test]
fn a_408_is_a_slow_query_only_on_query_run() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let timeout = json!({ "detail": "Request timeout", "remaining_job_ids": [job] }).to_string();
    let models = server_only(vec![Exchange::new("GET", MODELS, 408, timeout.clone())]);
    let error = client(&models).topics().expect_err("408");
    assert!(
        matches!(
            error,
            OmniError::Status {
                endpoint: Endpoint::ListModels,
                status: 408,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("GET /v1/models answered 408"),
        "{error}"
    );
    let server = server(vec![
        Exchange::new("POST", "/v1/query/run", 408, timeout.clone()).once(),
        Exchange::new(
            "GET",
            &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
            408,
            timeout,
        ),
    ]);
    let error = client(&server)
        .query(&revenue_by_region())
        .expect_err("408");
    assert!(
        matches!(
            error,
            OmniError::Status {
                endpoint: Endpoint::WaitForQuery,
                status: 408,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn hostile_job_ids_are_never_sent() {
    let timeout = json!({ "remaining_job_ids": ["x&jobIds=../../models"], "timed_out": true });
    let server = server(vec![Exchange::new(
        "POST",
        "/v1/query/run",
        408,
        timeout.to_string(),
    )]);
    let error = client(&server)
        .query(&revenue_by_region())
        .expect_err("hostile job id");
    assert!(matches!(
        error,
        OmniError::Schema {
            endpoint: Endpoint::WaitForQuery,
            ..
        }
    ));
    assert!(
        server
            .seen()
            .iter()
            .all(|s| !s.path.starts_with("/v1/query/wait"))
    );
}

#[test]
fn a_call_stops_at_its_time_budget() {
    let slow = Exchange::new("GET", MODELS, 200, "{}").delayed(Duration::from_secs(3));
    let server = server_only(vec![slow]);
    let started = std::time::Instant::now();
    let error = client(&server)
        .with_budget(Duration::from_millis(500))
        .topics()
        .expect_err("slow");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert!(
        matches!(
            error,
            OmniError::Deadline { .. }
                | OmniError::Transport {
                    kind: TransportKind::Timeout,
                    ..
                }
        ),
        "{error:?}"
    );
}

#[test]
fn omni_error_details_are_plain_short_and_redacted() {
    let detail = format!(
        "bad key {KEY} \u{1b}]52;c;ZXZpbA==\u{7} {}",
        "z".repeat(500)
    );
    let body = json!({ "detail": detail, "status": 400 }).to_string();
    let server = server_only(vec![Exchange::new("GET", MODELS, 400, body)]);
    let error = client(&server).topics().expect_err("400");
    let OmniError::Status {
        status: 400,
        detail: Some(detail),
        ..
    } = &error
    else {
        panic!("{error:?}");
    };
    assert!(detail.starts_with("bad key [redacted]"), "{detail}");
    assert!(!detail.contains('\u{1b}'));
    assert!(detail.chars().count() <= 200);
}

/// ADR-008 and definition of done point 21: a planted key appears in no error,
/// no `Debug` output and no response detail, on every failure path.
#[test]
fn the_planted_key_appears_in_no_output() {
    let job = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let echo =
        json!({ "detail": format!("Authorization: Bearer {KEY}"), "message": KEY }).to_string();
    let scenarios: Vec<Vec<Exchange>> = vec![
        vec![],
        vec![Exchange::new("GET", MODELS, 401, echo.clone())],
        vec![Exchange::new("GET", MODELS, 500, echo.clone())],
        vec![Exchange::new("GET", MODELS, 200, echo.clone())],
        vec![
            Exchange::new("GET", MODELS, 302, "")
                .header("Location", &format!("https://x.test/?k={KEY}")),
        ],
        vec![Exchange::new("POST", "/v1/query/run", 400, echo.clone())],
        vec![Exchange::new(
            "POST",
            "/v1/query/run",
            408,
            json!({ "remaining_job_ids": [KEY] }).to_string(),
        )],
        vec![
            Exchange::new(
                "POST",
                "/v1/query/run",
                408,
                json!({ "remaining_job_ids": [job] }).to_string(),
            ),
            Exchange::new(
                "GET",
                &format!("/v1/query/wait?jobIds={job}&completionOnly=true"),
                200,
                format!(
                    "{{\"status\":\"FAILED\",\"error_type\":\"{KEY}\"}}\n{{\"remaining_job_ids\":[],\"timed_out\":\"false\"}}"
                ),
            ),
        ],
    ];
    let mut outputs = Vec::new();
    for extra in scenarios {
        let server = server(extra);
        let omni = client(&server);
        outputs.push(format!("{omni:?}"));
        for result in [
            omni.topics().map(|t| format!("{t:?}")),
            omni.fields("orders").map(|f| format!("{f:?}")),
            omni.fields("returns").map(|f| format!("{f:?}")),
            omni.query(&revenue_by_region()).map(|t| format!("{t:?}")),
            omni.query(&source(&["orders.nope"]))
                .map(|t| format!("{t:?}")),
        ] {
            match result {
                Ok(text) => outputs.push(text),
                Err(error) => outputs.push(format!("{error} {error:?}")),
            }
        }
    }
    for output in &outputs {
        assert!(!output.contains(KEY), "{output}");
    }
    assert!(outputs.iter().any(|o| o.contains("[redacted]")));
}

/// The production path: `https://` with rustls, ring and the platform
/// verifier. Pointed at the plain-HTTP replay server, the handshake must fail
/// cleanly, never fall back to plain HTTP, and never panic.
#[test]
fn the_https_path_refuses_a_server_without_tls() {
    let server = server(vec![]);
    let mut config = server.config();
    let port = config
        .base
        .authority()
        .rsplit(':')
        .next()
        .expect("port")
        .to_owned();
    config.base = tiles_omni::BaseUrl::parse(&format!("https://127.0.0.1:{port}")).expect("url");
    let error = OmniClient::new(config)
        .with_budget(Duration::from_secs(10))
        .topics()
        .expect_err("no TLS");
    assert!(
        matches!(
            error,
            OmniError::Transport {
                endpoint: Endpoint::ListModels,
                kind: TransportKind::Tls | TransportKind::Protocol | TransportKind::Connect
            }
        ),
        "{error:?}"
    );
    assert!(server.seen().iter().all(|s| !s.recorded));
}

/// A self-signed certificate for `localhost` and 127.0.0.1, valid until 2126.
/// No trust store holds it, so it stands in for a TLS-inspecting proxy whose
/// root the machine does not trust.
const UNTRUSTED_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIBmzCCAUGgAwIBAgIURWLbP7LdsLUeMqUwkWa84WRdCv4wCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MTAxNzIzMjMxN1oYDzIxMjYwOTIz
MjMyMzE3WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAAROVVqGy7L4C7GOSzlAnWIJZIB6b9TJ93tywMHRaFoJj5QfJLVEdthx
dFUKpdomlXo6py3dr+2+NYtwSkk/j1ybo28wbTAdBgNVHQ4EFgQUvqhoQlkAK1Xb
862NtKM9uJiosU8wHwYDVR0jBBgwFoAUvqhoQlkAK1Xb862NtKM9uJiosU8wDwYD
VR0TAQH/BAUwAwEB/zAaBgNVHREEEzARgglsb2NhbGhvc3SHBH8AAAEwCgYIKoZI
zj0EAwIDSAAwRQIhALp2XPVZNwU2TaNXRYds6XpgJ+vG72GI9riskClW1OmcAiA7
WdFjWokoS/EIiZc2/5ym5Z5zupNSg94iQCuZPLsbvg==
-----END CERTIFICATE-----
";
const UNTRUSTED_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgPd1wK2AxQk9UUGOZ
Hucs40q1YJoH49ErW6QyiLPVM6qhRANCAAROVVqGy7L4C7GOSzlAnWIJZIB6b9TJ
93tywMHRaFoJj5QfJLVEdthxdFUKpdomlXo6py3dr+2+NYtwSkk/j1yb
-----END PRIVATE KEY-----
";

/// A rejected certificate is a TLS failure, reported as one and not retried
/// as a network problem, and the server never sees a request.
#[test]
fn an_untrusted_certificate_is_a_tls_error() {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::{Arc, mpsc};

    let tls = Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from_pem_slice(UNTRUSTED_CERT.as_bytes()).expect("cert")],
            PrivateKeyDer::from_pem_slice(UNTRUSTED_KEY.as_bytes()).expect("key"),
        )
        .expect("server config"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, handshakes) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut conn = rustls::ServerConnection::new(Arc::clone(&tls)).expect("connection");
            let mut tls_stream = rustls::Stream::new(&mut conn, &mut stream);
            let mut request = Vec::new();
            let read = tls_stream.read_to_end(&mut request).map(|_| request);
            if tx.send(read).is_err() {
                return;
            }
        }
    });

    let server = server(vec![]);
    let mut config = server.config();
    config.base = tiles_omni::BaseUrl::parse(&format!("https://localhost:{port}")).expect("url");
    let error = OmniClient::new(config)
        .with_budget(Duration::from_secs(10))
        .topics()
        .expect_err("untrusted certificate");
    assert!(
        matches!(
            error,
            OmniError::Transport {
                endpoint: Endpoint::ListModels,
                kind: TransportKind::Tls
            }
        ),
        "{error:?}"
    );
    let first = handshakes
        .recv_timeout(Duration::from_secs(5))
        .expect("one handshake");
    assert!(first.is_err(), "the server read a request: {first:?}");
    assert!(
        handshakes.recv_timeout(Duration::from_secs(2)).is_err(),
        "the handshake was retried"
    );
}
