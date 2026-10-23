//! `tiles omni`: store the URL and key, list topics, list a topic's fields,
//! and fetch an intent's rows (ADR-008). Every line printed goes through the
//! one sanitizer (ADR-009), and `rows` prints the shape of the result, never
//! its values (ADR-005).

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use serde_json::Value;
use tiles_intent::source::{DataSource, OmniSource, Table};
use tiles_intent::text::{sanitize, sanitize_truncated};
use tiles_omni::{
    ApiKey, BaseUrl, FieldKind, Keychain, MAX_KEY_BYTES, OmniClient, OmniConfig, OmniError,
    SecretStore,
};

/// Largest intent file `tiles omni rows` reads (ADR-009's 2 MB payload limit).
const MAX_INTENT_BYTES: u64 = 2 * 1024 * 1024;

const USAGE: &str = "usage: tiles omni set-key <https://yourcompany.omniapp.co>
       tiles omni topics
       tiles omni fields <topic>
       tiles omni rows <intent.json | ->

set-key reads the API key with echo off and stores it in the login Keychain,
readable without a prompt only by this tiles binary, then sets the Tiles tools
of every install Tiles made to ask before each call. Otherwise Tiles reads
TILES_OMNI_URL and TILES_OMNI_API_KEY, both from the environment (docs/omni.md).";

/// Stores the URL and the key read from `key_input` in `store`, then gives
/// the harness installs Tiles made under `dirs` the approval settings an
/// install with Omni writes ([`crate::install::reapply_approvals`]), and
/// returns what to tell the user, line by line. The key is checked before
/// anything is stored.
///
/// # Errors
///
/// A refused URL or key, or a refused write; never holding the key.
pub fn set_key(
    url: &str,
    key_input: &mut dyn Read,
    store: &dyn SecretStore,
    dirs: Option<&crate::install::Dirs>,
) -> Result<Vec<String>, String> {
    let base = BaseUrl::parse(url).map_err(|e| e.to_string())?;
    // Sized for the longest key, so reading it leaves no partial copy behind.
    let mut raw = String::with_capacity(MAX_KEY_BYTES + 2);
    let limit = u64::try_from(MAX_KEY_BYTES + 2).unwrap_or(u64::MAX);
    key_input
        .take(limit)
        .read_to_string(&mut raw)
        .map_err(|_| "cannot read the API key".to_string())?;
    let first = raw.lines().next().unwrap_or_default();
    let key = ApiKey::new(first).map_err(|e| format!("the Omni API key {e}"));
    tiles_omni::wipe(raw);
    let key = key?;
    tiles_omni::store_settings(store, &base, &key).map_err(|e| e.to_string())?;
    let host = base.authority();
    let mut said = vec![format!(
        "stored the Omni URL and API key for {host} in the login Keychain; only this tiles binary reads them without a macOS prompt. \
         To check, run: security find-generic-password -s tiles.omni -a api-key@{host} -w, which should ask first (choose Deny)"
    )];
    said.extend(
        dirs.map(|d| {
            crate::install::reapply_approvals(
                d,
                &[tiles_mcp::OMNI_TOPICS_TOOL, tiles_mcp::OMNI_FIELDS_TOOL],
                None,
            )
        })
        .unwrap_or_default(),
    );
    Ok(said)
}

/// Reads one line from the terminal with echo off, or all of standard input
/// when it is not a terminal.
fn hidden_key(host: &str) -> io::Result<String> {
    let stdin = rustix::stdio::stdin();
    let saved = rustix::termios::tcgetattr(stdin).ok();
    if let Some(saved) = &saved {
        eprint!("Omni API key for {} (input hidden): ", sanitize(host));
        let mut quiet = saved.clone();
        quiet.local_modes.remove(rustix::termios::LocalModes::ECHO);
        let _ = rustix::termios::tcsetattr(stdin, rustix::termios::OptionalActions::Now, &quiet);
    }
    let mut line = String::with_capacity(MAX_KEY_BYTES + 2);
    let read = io::stdin().read_line(&mut line);
    if let Some(saved) = &saved {
        let _ = rustix::termios::tcsetattr(stdin, rustix::termios::OptionalActions::Now, saved);
        eprintln!();
    }
    match read {
        Ok(_) => Ok(line),
        Err(e) => {
            tiles_omni::wipe(line);
            Err(e)
        }
    }
}

/// Runs `tiles omni` with the process's configuration and standard streams.
pub fn main(args: &[String]) -> ExitCode {
    if let [command, url] = args
        && command == "set-key"
    {
        let host = BaseUrl::parse(url)
            .map(|b| b.authority().to_owned())
            .unwrap_or_default();
        let result = hidden_key(&host)
            .map_err(|_| "cannot read the API key".to_string())
            .and_then(|key| {
                let dirs = crate::install::Dirs::from_env(&crate::serve::env);
                let outcome = set_key(url, &mut key.as_bytes(), &Keychain, dirs.as_ref());
                tiles_omni::wipe(key);
                outcome
            });
        return match result {
            Ok(done) => {
                for said in done {
                    line(&mut io::stdout(), &format!("tiles: {said}"));
                }
                ExitCode::SUCCESS
            }
            Err(problem) => {
                line(&mut io::stderr(), &format!("tiles: {problem}"));
                ExitCode::FAILURE
            }
        };
    }
    let connect = || {
        OmniConfig::from_system()
            .map(OmniClient::new)
            .map_err(OmniError::from)
    };
    let stdin = io::stdin();
    let code = run(
        args,
        &connect,
        &mut stdin.lock(),
        &mut io::stdout(),
        &mut io::stderr(),
    );
    ExitCode::from(code)
}

/// Runs `tiles omni <args>`, connecting through `connect` only when a command
/// needs Omni. Returns the exit code: 0 done, 1 failed, 2 misused.
pub fn run(
    args: &[String],
    connect: &dyn Fn() -> Result<OmniClient, OmniError>,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["topics"] => connect().and_then(|omni| topics(&omni, out)),
        ["fields", topic] => connect().and_then(|omni| fields(&omni, topic, out)),
        ["rows", path] => match read_source(path, stdin) {
            Ok(source) => rows(&source, connect, out),
            Err(problem) => {
                line(err, &format!("tiles: {problem}"));
                return 1;
            }
        },
        [] | ["help" | "--help" | "-h"] => {
            line(out, USAGE);
            return 0;
        }
        _ => {
            line(err, USAGE);
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            line(err, &format!("tiles: {error}"));
            1
        }
    }
}

/// Writes `text` made plain, line by line. Output errors (a closed pipe) are
/// ignored: there is nowhere left to report them.
fn line(out: &mut dyn Write, text: &str) {
    for part in text.split('\n') {
        let _ = writeln!(out, "{}", sanitize(part));
    }
}

fn plain(text: Option<&str>, max: usize) -> String {
    text.map(|t| sanitize_truncated(t.trim(), max))
        .unwrap_or_default()
}

pub(crate) fn topics(omni: &OmniClient, out: &mut dyn Write) -> Result<(), OmniError> {
    let listing = omni.topics()?;
    let width = listing
        .topics
        .iter()
        .map(|t| t.name.len())
        .max()
        .unwrap_or(0);
    let models: std::collections::HashSet<&str> =
        listing.topics.iter().map(|t| t.model.as_str()).collect();
    let count = listing.topics.len();
    line(
        out,
        &format!("{count} topic{} in Omni", if count == 1 { "" } else { "s" }),
    );
    for topic in &listing.topics {
        let mut text = format!(
            "  {:width$}  {}",
            topic.name,
            plain(topic.label.as_deref(), 60)
        );
        let description = plain(topic.description.as_deref(), 80);
        if !description.is_empty() {
            text.push_str(" · ");
            text.push_str(&description);
        }
        if models.len() > 1 {
            let _ = write!(text, "  (model {})", topic.model);
        }
        line(out, &text);
    }
    if listing.skipped > 0 {
        line(
            out,
            &format!(
                "{} topics with names Tiles cannot query were left out",
                listing.skipped
            ),
        );
    }
    Ok(())
}

pub(crate) fn fields(omni: &OmniClient, topic: &str, out: &mut dyn Write) -> Result<(), OmniError> {
    let topic = omni.fields(topic)?;
    let width = topic.fields.iter().map(|f| f.name.len()).max().unwrap_or(0);
    line(
        out,
        &format!("{}: {} fields", topic.topic, topic.fields.len()),
    );
    for (kind, heading) in [
        (FieldKind::Dimension, "dimensions"),
        (FieldKind::Measure, "measures"),
    ] {
        let group: Vec<_> = topic.fields.iter().filter(|f| f.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        line(out, &format!("  {heading}"));
        for field in group {
            let kind = match field.kind {
                FieldKind::Dimension => field.data_type.as_deref(),
                FieldKind::Measure => field.aggregate.as_deref().or(field.data_type.as_deref()),
            };
            let mut text = format!(
                "    {:width$}  {:9}  {}",
                field.name,
                plain(kind, 9).to_ascii_lowercase(),
                plain(field.label.as_deref(), 50)
            );
            if !field.time_frames.is_empty() {
                let _ = write!(text, "  [{}]", field.time_frames.join(" "));
            }
            line(out, &text);
        }
    }
    if topic.skipped > 0 {
        line(
            out,
            &format!(
                "{} fields with names Tiles cannot query were left out",
                topic.skipped
            ),
        );
    }
    Ok(())
}

/// The `data.omni` of a whole intent, from a file or standard input.
fn read_source(path: &str, stdin: &mut dyn Read) -> Result<OmniSource, String> {
    let mut bytes = Vec::new();
    let read = if path == "-" {
        stdin.take(MAX_INTENT_BYTES + 1).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(path).and_then(|f| f.take(MAX_INTENT_BYTES + 1).read_to_end(&mut bytes))
    };
    if let Err(error) = read {
        return Err(format!("cannot read the intent: {}", error.kind()));
    }
    if bytes.len() as u64 > MAX_INTENT_BYTES {
        return Err("the intent is over 2 MB".to_owned());
    }
    let mut value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("the intent is not JSON (line {})", e.line()))?;
    let Some(data) = value.get_mut("data").map(Value::take) else {
        return Err("the intent has no `data` key".to_owned());
    };
    match serde_json::from_value(data) {
        Ok(DataSource::Omni(source)) => Ok(source),
        Ok(DataSource::Rows(_)) => Err(
            "data.rows needs no fetching; `tiles omni rows` reads only data.omni, built from `tiles omni fields <topic>`"
                .to_owned(),
        ),
        Ok(DataSource::Warehouse(_)) => Err(
            "data.warehouse is a warehouse query; `tiles omni rows` reads only data.omni, built from `tiles omni fields <topic>`"
                .to_owned(),
        ),
        Err(error) => Err(format!(
            "data must be exactly one of {{\"rows\": [...]}} or {{\"omni\": {{\"topic\", \"fields\", \"sorts\", \"limit\"}}}} ({})",
            sanitize_truncated(&error.to_string(), 160)
        )),
    }
}

fn rows(
    source: &OmniSource,
    connect: &dyn Fn() -> Result<OmniClient, OmniError>,
    out: &mut dyn Write,
) -> Result<(), OmniError> {
    source.validate()?;
    let table = connect()?.query(source)?;
    shape(&table, &format!("Omni topic {}", source.topic), out);
    Ok(())
}

/// The table's shape: row count and each column's kind. Values stay out of
/// the output: rows go to the engine, not to the model (ADR-005).
fn shape(table: &Table, origin: &str, out: &mut dyn Write) {
    let count = table.rows().len();
    line(
        out,
        &format!(
            "{count} row{} from {origin}",
            if count == 1 { "" } else { "s" }
        ),
    );
    let width = table
        .columns()
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0);
    for column in table.columns() {
        line(
            out,
            &format!("  {:width$}  {}", column.name, column.kind.name()),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;
    use tiles_omni::replay::{Exchange, ReplayServer, recorded};
    use tiles_omni::{OmniClient, OmniError};

    use super::run;

    const KEY: &str = "omni_planted_key_7Yq2xL9vR4";
    const MODEL: &str = "4f8c2a1e-0b7d-4c55-9a3e-2d1f6b8e9c01";

    struct Outcome {
        code: u8,
        out: String,
        err: String,
    }

    fn tiles(server: &ReplayServer, args: &[&str], stdin: &str) -> Outcome {
        let connect = || Ok::<_, OmniError>(OmniClient::new(server.config()));
        drive(&connect, args, stdin)
    }

    fn drive(
        connect: &dyn Fn() -> Result<OmniClient, OmniError>,
        args: &[&str],
        stdin: &str,
    ) -> Outcome {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, connect, &mut stdin.as_bytes(), &mut out, &mut err);
        Outcome {
            code,
            out: String::from_utf8(out).expect("utf-8"),
            err: String::from_utf8(err).expect("utf-8"),
        }
    }

    fn server(extra: Vec<Exchange>) -> ReplayServer {
        ReplayServer::start(KEY, extra.into_iter().chain(recorded()).collect())
    }

    #[test]
    fn topics_prints_one_line_per_topic() {
        let server = server(vec![]);
        let result = tiles(&server, &["topics"], "");
        assert_eq!(result.code, 0, "{}", result.err);
        let lines: Vec<&str> = result.out.lines().collect();
        assert_eq!(lines[0], "2 topics in Omni");
        assert!(
            lines[1].starts_with("  orders         Orders · Every café"),
            "{}",
            lines[1]
        );
        assert!(
            lines[2].starts_with("  subscriptions  Subscriptions"),
            "{}",
            lines[2]
        );
        assert_eq!(server.unrecorded(), []);
    }

    #[test]
    fn fields_groups_dimensions_and_measures() {
        let server = server(vec![]);
        let result = tiles(&server, &["fields", "orders"], "");
        assert_eq!(result.code, 0, "{}", result.err);
        assert!(
            result.out.starts_with("orders: 6 fields\n  dimensions\n"),
            "{}",
            result.out
        );
        assert!(
            result.out.contains(
                "orders.created_at  timestamp  Created At  [date week month quarter year]"
            )
        );
        assert!(result.out.contains("  measures\n"));
        assert!(
            result.out.contains("orders.revenue     sum        Revenue"),
            "{}",
            result.out
        );
    }

    #[test]
    fn rows_prints_the_shape_but_no_values() {
        let server = server(vec![]);
        let intent = json!({
            "v": 1,
            "form": "bar",
            "title": "Pacific Northwest leads revenue",
            "data": { "omni": {
                "topic": "orders",
                "fields": ["orders.region", "orders.revenue"],
                "sorts": [{ "field": "orders.revenue", "desc": true }],
                "limit": 5
            }}
        });
        let result = tiles(&server, &["rows", "-"], &intent.to_string());
        assert_eq!(result.code, 0, "{}", result.err);
        assert_eq!(
            result.out,
            "5 rows from Omni topic orders\n  orders.region   text\n  orders.revenue  number\n"
        );
        assert!(!result.out.contains("Pacific"));
    }

    #[test]
    fn rows_reads_an_intent_file() {
        let server = server(vec![]);
        let dir = std::env::temp_dir().join(format!("tiles-cli-omni-rows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("intent.json");
        let mut file = std::fs::File::create(&path).expect("intent file");
        write!(
            file,
            r#"{{"data": {{"omni": {{"topic": "orders", "fields": ["orders.created_at[week]", "orders.count"], "sorts": [{{"field": "orders.created_at[week]"}}], "limit": 12}}}}}}"#
        )
        .expect("write");
        let result = tiles(&server, &["rows", path.to_str().expect("path")], "");
        std::fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(result.code, 0, "{}", result.err);
        assert_eq!(
            result.out,
            "4 rows from Omni topic orders\n  orders.created_at[week]  text\n  orders.count             number\n"
        );
    }

    #[test]
    fn a_bad_intent_is_refused_before_connecting() {
        let refuse = || -> Result<OmniClient, OmniError> { panic!("must not connect") };
        let huge = format!(
            "{{\"data\":{{\"rows\":[{{\"a\":\"{}\"}}]}}}}",
            "x".repeat(2 * 1024 * 1024)
        );
        let cases = [
            ("not json", "not JSON"),
            (
                r#"{"data": {"omni": {"topic": "orders", "fields": ["orders.count"], "filters": {}}}}"#,
                "exactly one of",
            ),
            (
                r#"{"data": {"omni": {"topic": "../x", "fields": ["orders.count"]}}}"#,
                "data.omni.topic",
            ),
            (
                r#"{"data": {"omni": {"topic": "orders", "fields": ["orders.count"], "limit": 0}}}"#,
                "data.omni.limit",
            ),
            (
                r#"{"data": {"rows": [{"a": "\u001b]52;c;ZXZpbA==\u0007"}]}}"#,
                "reads only data.omni",
            ),
            (
                r#"{"omni": {"topic": "orders", "fields": ["orders.count"]}}"#,
                "no `data` key",
            ),
            (huge.as_str(), "over 2 MB"),
        ];
        for (stdin, expected) in cases {
            let result = drive(&refuse, &["rows", "-"], stdin);
            assert_eq!(result.code, 1, "{expected}");
            assert!(result.err.contains(expected), "{expected}: {}", result.err);
            assert!(!result.err.contains('\u{1b}'));
        }
        assert_eq!(
            drive(&refuse, &["rows", "/nonexistent/intent.json"], "").code,
            1
        );
    }

    struct Store(std::cell::RefCell<Vec<(String, String)>>);

    impl tiles_omni::SecretStore for Store {
        fn get(&self, account: &str) -> Result<Option<String>, tiles_omni::ConfigError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|(a, _)| a == account)
                .map(|(_, v)| v.clone()))
        }
        fn put(&self, account: &str, value: &str) -> Result<(), tiles_omni::ConfigError> {
            self.0.borrow_mut().push((account.into(), value.into()));
            Ok(())
        }
    }

    #[test]
    fn set_key_stores_the_key_under_its_host_and_never_prints_it() {
        let store = Store(std::cell::RefCell::new(Vec::new()));
        let said = super::set_key(
            "https://acme.omniapp.co",
            &mut format!("{KEY}\n").as_bytes(),
            &store,
            None,
        )
        .expect("stored")
        .join("\n");
        assert!(!said.contains(KEY) && said.contains("acme.omniapp.co"));
        assert_eq!(
            *store.0.borrow(),
            [
                ("url".to_string(), "https://acme.omniapp.co/api".to_string()),
                ("api-key@acme.omniapp.co".to_string(), KEY.to_string())
            ]
        );
        let empty = Store(std::cell::RefCell::new(Vec::new()));
        let bad = super::set_key("http://acme.omniapp.co", &mut KEY.as_bytes(), &empty, None)
            .expect_err("plain http");
        assert!(bad.contains("https://") && !bad.contains(KEY));
        let bad = super::set_key(
            "https://acme.omniapp.co",
            &mut "a b\n".as_bytes(),
            &empty,
            None,
        )
        .expect_err("a key with a space");
        assert!(!bad.contains("a b"));
        assert!(empty.0.borrow().is_empty(), "nothing stored on a refusal");
    }

    #[test]
    fn misuse_prints_usage_and_exits_2() {
        let refuse = || -> Result<OmniClient, OmniError> { panic!("must not connect") };
        for args in [&["bogus"][..], &["fields"], &["topics", "extra"]] {
            let result = drive(&refuse, args, "");
            assert_eq!(result.code, 2);
            assert!(result.err.starts_with("usage: tiles omni set-key"));
        }
        assert_eq!(drive(&refuse, &["--help"], "").code, 0);
    }

    #[test]
    fn hostile_labels_from_omni_reach_the_terminal_plain() {
        let topics = json!({ "topics": [{
            "name": "orders",
            "label": "Ord\u{1b}]52;c;ZXZpbA==\u{7}ers\u{202E}",
            "description": "line one\nline two\u{1b}[2J\u{9b}31m"
        }]});
        let server = server(vec![Exchange::new(
            "GET",
            &format!("/v1/models/{MODEL}/topic"),
            200,
            topics.to_string(),
        )]);
        let result = tiles(&server, &["topics"], "");
        assert_eq!(result.code, 0, "{}", result.err);
        assert_eq!(result.out.lines().count(), 2, "{}", result.out);
        for c in result.out.chars() {
            assert!(
                c == '\n' || !tiles_intent::text::is_unsafe_char(c),
                "{:?}",
                c as u32
            );
        }
    }

    /// Definition of done point 21: the planted key reaches no output of any
    /// `tiles omni` command, on success or failure.
    #[test]
    fn the_planted_key_reaches_no_command_output() {
        let echo = json!({ "detail": format!("bad token {KEY}") }).to_string();
        let runs: Vec<(Vec<Exchange>, Vec<&str>)> = vec![
            (vec![], vec!["topics"]),
            (vec![], vec!["fields", "orders"]),
            (vec![], vec!["fields", "missing"]),
            (
                vec![Exchange::new(
                    "GET",
                    "/v1/models?modelKind=SHARED&pageSize=100",
                    401,
                    echo.clone(),
                )],
                vec!["topics"],
            ),
            (
                vec![Exchange::new("POST", "/v1/query/run", 400, echo.clone())],
                vec!["rows", "-"],
            ),
        ];
        let intent = r#"{"data":{"omni":{"topic":"orders","fields":["orders.region","orders.revenue"],"sorts":[{"field":"orders.revenue","desc":true}],"limit":5}}}"#;
        for (extra, args) in runs {
            let server = server(extra);
            let result = tiles(&server, &args, intent);
            assert!(
                !result.out.contains(KEY) && !result.err.contains(KEY),
                "{args:?}: {}{}",
                result.out,
                result.err
            );
        }
        let wrong = ReplayServer::start("another_key", recorded());
        let connect = || {
            let mut config = wrong.config();
            config.key = tiles_omni::ApiKey::new(KEY).expect("key");
            Ok::<_, OmniError>(OmniClient::new(config))
        };
        let result = drive(&connect, &["topics"], "");
        assert!(!result.out.contains(KEY) && !result.err.contains(KEY));
        assert_eq!(result.code, 1);
        assert!(result.err.contains("401"), "{}", result.err);
    }
}
