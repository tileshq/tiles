use super::{
    Command, Render, USAGE, parse, parse_args, refusal, render, source_command, source_lines,
    usage, version_line, write_atomic,
};
use tiles_engine::{Options, Theme};
use tiles_mcp::source::Source;

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn version_line_names_the_schema() {
    assert!(version_line().starts_with("tiles "));
    assert!(version_line().contains("intent schema 1"));
}

#[test]
fn render_parses_its_options() {
    let Ok(Command::Render(r)) = parse_args(&args(
        "render in.json -o out.png --theme dark --width 800 --scale 3",
    )) else {
        panic!("render should parse");
    };
    assert_eq!(r.input, "in.json");
    assert_eq!(r.output.to_str(), Some("out.png"));
    assert_eq!(r.opts.theme, Theme::from_name("dark").unwrap());
    assert_eq!(r.opts.width, 800.0);
    assert_eq!(r.scale, 3.0);
    let default = Theme::default();
    let Ok(Command::Render(r)) = parse_args(&args(&format!(
        "render in.json -o x --theme {}",
        default.name()
    ))) else {
        panic!("the default theme's name should parse");
    };
    assert_eq!(r.opts.theme, default);
    assert!(r.themed);
    let Ok(Command::Render(r)) = parse_args(&args("render in.json -o x")) else {
        panic!("no theme should parse");
    };
    assert!(!r.themed, "the settings decide");
}

#[test]
fn theme_and_open_parse() {
    assert_eq!(parse_args(&args("theme")), Ok(Command::Theme(None)));
    assert_eq!(
        parse_args(&args("theme dark")),
        Ok(Command::Theme(Some("dark".into())))
    );
    assert_eq!(
        parse_args(&args("open a1b2c3")),
        Ok(Command::Open("a1b2c3".into()))
    );
    assert!(parse_args(&args("theme dark light")).is_err());
    assert!(parse_args(&args("open")).is_err());
    assert_eq!(parse_args(&args("cache clear")), Ok(Command::CacheClear));
    for wrong in ["cache", "cache empty", "cache clear now"] {
        assert_eq!(
            parse_args(&args(wrong)),
            Err("usage: tiles cache clear".into()),
            "{wrong}"
        );
    }
}

#[test]
fn a_long_refusal_is_printed_whole_wrapped_and_clean() {
    let command =
        "`DBT_MR_DATASET=dbt_mr_1234 dbt parse --target mr --target-path target/tiles/mr-1234`";
    let path = "`/Users/ada/Projects/analytics/dbt/target/tiles/mr-1234/manifest.json`";
    let hint = format!(
        "No manifest for MR 1234's partial build. At the MR's commit, run {command} from the dbt project directory `/Users/ada/Projects/analytics/dbt`, then ask again; it writes {path} \u{1b}]52;c;eA==\u{7}end"
    );
    assert!(hint.chars().count() > 300, "longer than the old cut");
    let plain = refusal(&hint, None);
    assert!(!plain.contains('\n'), "not a terminal: one line");
    assert!(plain.starts_with("tiles: No manifest for MR 1234's partial build."));
    assert!(plain.ends_with("?]52;c;eA==?end"), "{plain}");
    let wrapped = refusal(&hint, Some(60));
    for text in [&plain, &wrapped] {
        assert!(!text.contains(['\u{1b}', '\u{7}']), "{text:?}");
    }
    let lines: Vec<&str> = wrapped.lines().collect();
    assert!(lines.len() > 4, "{wrapped}");
    assert!(lines[0].starts_with("tiles: "));
    for line in &lines[1..] {
        assert!(
            line.starts_with("       ") && !line.starts_with("        "),
            "{line:?}"
        );
    }
    let whole = |l: &&str| l.trim() == command || l.trim() == path;
    for line in &lines {
        assert!(line.chars().count() <= 60 || whole(line), "{line:?}");
    }
    assert!(
        lines.iter().any(|l| l.trim() == command),
        "the command keeps one line: {wrapped}"
    );
    assert!(
        lines.iter().any(|l| l.trim() == path),
        "the path is never split: {wrapped}"
    );
    let rejoined: Vec<&str> = wrapped
        .trim_start_matches("tiles: ")
        .split_whitespace()
        .collect();
    assert_eq!(
        rejoined,
        plain
            .trim_start_matches("tiles: ")
            .split_whitespace()
            .collect::<Vec<_>>(),
        "every word is printed"
    );
    assert!(refusal(&"x ".repeat(5_000), None).ends_with('…'));
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        ))
        .unwrap(),
    )
    .unwrap();
    for e in corpus["entries"].as_array().unwrap() {
        let e = e.as_str().unwrap();
        for cols in [None, Some(40), Some(160)] {
            let text = refusal(&format!("{e} {e} then ask again"), cols);
            assert!(
                text.lines()
                    .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char)),
                "{cols:?} {e:?}"
            );
            assert!(cols.is_some() || !text.contains('\n'), "{e:?}");
        }
    }
}

#[test]
fn a_path_in_backticks_never_splits_even_with_spaces() {
    let project = "`/Users/ada/Library/Mobile Documents/com~apple~CloudDocs/analytics/dbt`";
    let path = "`/Users/ada/Library/Mobile Documents/com~apple~CloudDocs/analytics/dbt/target/tiles/mr-1234/manifest.json`";
    let hint = format!(
        "No manifest for MR 1234's partial build. At the MR's commit, run `DBT_MR_DATASET=dbt_mr_1234 dbt parse --target mr` from the dbt project directory {project}, then ask again; it writes {path}"
    );
    let wrapped = refusal(&hint, Some(80));
    let lines: Vec<&str> = wrapped.lines().collect();
    assert!(lines.len() > 3, "{wrapped}");
    for span in [project, path] {
        assert!(
            lines.iter().any(|l| l.contains(span)),
            "{span} on one line: {wrapped}"
        );
    }
    for line in &lines {
        assert!(
            line.chars().count() <= 80
                || line.trim().trim_end_matches([',', ';']) == project
                || line.trim() == path,
            "{line:?}"
        );
    }
}

#[test]
fn an_unmatched_backtick_wraps_as_plain_words() {
    let text = format!(
        "BigQuery said: Syntax error: Unexpected ` at [1:8] {}then ask again",
        "near the end of the query ".repeat(6)
    );
    let wrapped = refusal(&text, Some(60));
    let lines: Vec<&str> = wrapped.lines().collect();
    assert!(lines.len() > 3, "{wrapped}");
    for line in &lines {
        assert!(line.chars().count() <= 60, "{line:?}");
    }
    assert_eq!(
        wrapped
            .trim_start_matches("tiles: ")
            .split_whitespace()
            .collect::<Vec<_>>(),
        text.split_whitespace().collect::<Vec<_>>(),
        "every word is printed"
    );
}

#[test]
fn usage_errors_are_one_line_and_clean() {
    for (a, want) in [
        ("render in.json", "needs -o"),
        ("render -o out.png", "needs an intent file"),
        ("render in.json -o x --theme blue", "is not a theme; use "),
        ("render in.json -o x --width wide", "not a number"),
        ("render in.json -o x --width NaN", "not a number"),
        ("render a b -o x", "one intent file"),
        ("render in.json -o x --bogus", "unknown option"),
        ("draw", "unknown command"),
    ] {
        let e = parse_args(&args(a)).unwrap_err();
        assert!(e.contains(want), "{a}: {e}");
        assert!(!e.contains('\n'));
    }
    let hostile = vec!["\u{1b}]52;c;aGk=\u{7}".to_string()];
    let e = parse_args(&hostile).unwrap_err();
    assert!(!e.chars().any(tiles_intent::forbidden), "{e:?}");
}

#[test]
fn writes_replace_the_target_and_leave_no_temporary_file() {
    let dir = std::env::temp_dir().join(format!("tiles-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("card.png");
    write_atomic(&out, b"one").unwrap();
    write_atomic(&out, b"two").unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"two");
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn render_writes_the_card_and_each_entrance_frame() {
    let dir = std::env::temp_dir().join(format!("tiles-cli-frames-{}", std::process::id()));
    let intent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tiles-engine/tests/intents/column.json");
    let r = Render {
        input: intent.to_string_lossy().into_owned(),
        output: dir.join("card.png"),
        opts: Options::default(),
        themed: true,
        scale: 0.5,
        frames: Some(dir.join("frames")),
        timings: false,
    };
    std::fs::create_dir_all(&dir).unwrap();
    let report = render(&r, &[]).unwrap();
    assert!(report.contains("10 frames"), "{report}");
    for i in 0..10 {
        let png = std::fs::read(dir.join(format!("frames/frame-{i:02}.png"))).unwrap();
        assert!(png.starts_with(b"\x89PNG"), "frame {i}");
    }
    assert_eq!(std::fs::read_dir(dir.join("frames")).unwrap().count(), 10);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A registered source that answers with whatever text it was given.
struct Echo(&'static str, String);

impl Source for Echo {
    fn name(&self) -> &'static str {
        self.0
    }

    fn usage(&self) -> &'static str {
        "tiles echo <text>"
    }

    fn doctor(&self) -> Vec<String> {
        vec![format!("echo: {}", self.1)]
    }

    fn command(&self, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            Err(format!("no text\n{}", self.1))
        } else {
            Ok(format!("{}\n{}", args.join(" "), self.1))
        }
    }
}

#[test]
fn a_registered_source_gets_its_command_usage_and_doctor_lines() {
    let echo = Echo("echo", "ok".into());
    let sources: [&dyn Source; 1] = [&echo];
    assert_eq!(
        parse(&args("echo hello there"), &["echo"]),
        Ok(Command::Source(0, args("hello there")))
    );
    assert_eq!(
        source_command(sources[0], &args("hello"), None),
        Ok("hello\nok".to_string())
    );
    assert!(usage(&sources).ends_with("\n       tiles echo <text>"));
    assert!(
        usage(&sources).contains("tiles run [--wrapper] [--allow-omni] [--allow-echo] [--terminal")
    );
    assert_eq!(source_lines(&sources), vec!["echo: ok".to_string()]);
    let unknown = parse(&args("draw"), &["echo"]).unwrap_err();
    assert!(
        unknown.ends_with("check, update, mcp, card or echo"),
        "{unknown}"
    );
    assert!(
        parse_args(&args("echo"))
            .unwrap_err()
            .ends_with("mcp or card")
    );
}

#[test]
fn update_takes_its_own_arguments() {
    assert_eq!(
        parse_args(&args("update --check")),
        Ok(Command::Update(args("--check")))
    );
    assert_eq!(parse_args(&args("update")), Ok(Command::Update(vec![])));
    assert!(USAGE.contains("\n       tiles update [--check] [--allow-older] [vX.Y.Z]\n"));
}

#[test]
fn a_source_never_takes_a_built_in_command() {
    assert_eq!(parse(&args("doctor"), &["doctor"]), Ok(Command::Doctor));
    assert!(parse(&args("doctor now"), &["doctor"]).is_err());
}

#[test]
fn a_sources_text_reaches_the_terminal_plain() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for entry in json["entries"].as_array().unwrap() {
        let entry = entry.as_str().unwrap().to_string();
        let echo = Echo("echo", entry.clone());
        let sources: [&dyn Source; 1] = [&echo];
        let shown = [
            source_command(sources[0], std::slice::from_ref(&entry), None).unwrap(),
            source_command(sources[0], &[], None).unwrap_err(),
            source_lines(&sources).join("\n"),
        ];
        for text in shown {
            let unsafe_char = text
                .chars()
                .find(|&c| c != '\n' && tiles_intent::forbidden(c));
            assert_eq!(unsafe_char, None, "{entry:?} gave {text:?}");
        }
    }
}

/// A source its own `setup` command configures, as `tiles warehouse setup`
/// does.
struct Settable(std::cell::Cell<bool>);

impl Source for Settable {
    fn name(&self) -> &'static str {
        "warehouse"
    }

    fn usage(&self) -> &'static str {
        "tiles warehouse setup"
    }

    fn doctor(&self) -> Vec<String> {
        Vec::new()
    }

    fn command(&self, _: &[String]) -> Result<String, String> {
        self.0.set(true);
        Ok("saved".into())
    }

    fn offer(&self) -> Option<tiles_mcp::source::Offer> {
        self.0.get().then(|| tiles_mcp::source::Offer {
            name: "warehouse",
            origin: "BigQuery",
            tools: vec![tiles_mcp::source::Tool {
                name: "tiles_warehouse_models",
                title: "t",
                description: "d",
                input: serde_json::json!({}),
            }],
            instruction: String::new(),
            chart_line: String::new(),
            tool_timeout: std::time::Duration::from_secs(300),
        })
    }
}

#[test]
fn a_source_its_command_configures_makes_every_install_ask() {
    let root = std::env::temp_dir().join(format!("tiles-cli-settable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dirs = crate::install::Dirs {
        home: root.clone(),
        config_home: root.join("xdg"),
        codex_home: root.join("codex"),
    };
    let opencode = crate::addons::opencode_config(
        "/opt/tiles",
        &[],
        &tiles_mcp::source::Reach::default(),
        &[],
        &root.join("xdg/opencode/tiles-instructions.md"),
    );
    crate::addons::write_file(
        &root.join("xdg/opencode/config.json"),
        opencode.to_string().as_bytes(),
    )
    .unwrap();
    crate::addons::write_file(
        &crate::install::pi_extension(&root).join("tool.json"),
        b"{}",
    )
    .unwrap();
    crate::addons::write_file(
        &root.join("codex/config.toml"),
        b"[mcp_servers.tiles]\ncommand = \"/opt/tiles\"\nargs = [\"mcp\"]\n",
    )
    .unwrap();
    let source = Settable(std::cell::Cell::new(false));
    let said = source_command(&source, &args("setup"), Some(&dirs)).unwrap();
    assert!(
        said.starts_with("saved\nset the Tiles tools to ask in "),
        "{said}"
    );
    assert!(
        said.contains("\nset tool_timeout_sec = 330 for Codex's tiles server in "),
        "Codex waits out the source's 300 s: {said}"
    );
    let codex: toml::Table = std::fs::read_to_string(root.join("codex/config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        codex["mcp_servers"]["tiles"]["default_tools_approval_mode"].as_str(),
        Some("prompt")
    );
    assert_eq!(
        codex["mcp_servers"]["tiles"]["tool_timeout_sec"].as_integer(),
        Some(330)
    );
    assert!(
        said.ends_with("\nPi's installed Tiles extension predates this and has no warehouse tools; run tiles install pi to add them"),
        "{said}"
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("xdg/opencode/config.json")).unwrap())
            .unwrap();
    assert_eq!(config["permission"]["tiles_tiles_chart"], "ask");
    assert_eq!(config["permission"]["tiles_tiles_warehouse_models"], "ask");
    assert_eq!(
        source_command(&source, &args("setup"), Some(&dirs)).unwrap(),
        "saved",
        "a source already configured changes nothing"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn render_fetches_a_warehouse_query_only_through_a_configured_source() {
    let dir = std::env::temp_dir().join(format!("tiles-cli-render-wh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let intent = dir.join("intent.json");
    std::fs::write(
        &intent,
        r#"{"v":1,"form":"column","title":"T","fields":{"x":"a","y":"b"},"data":{"warehouse":{"sql":"SELECT 'x' AS a, 1 AS b"}}}"#,
    )
    .unwrap();
    let r = Render {
        input: intent.to_string_lossy().into_owned(),
        output: dir.join("card.png"),
        opts: Options::default(),
        themed: true,
        scale: 0.5,
        frames: None,
        timings: false,
    };
    let e = render(&r, &[]).unwrap_err();
    assert_eq!(
        e,
        "warehouse: no warehouse is configured for Tiles; tiles doctor says how to set one up"
    );
    let unconfigured = Settable(std::cell::Cell::new(false));
    assert_eq!(render(&r, &[&unconfigured]).unwrap_err(), e);
    assert!(!dir.join("card.png").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pi_lists_a_sources_tools_only_while_it_is_configured() {
    let source = Settable(std::cell::Cell::new(false));
    let tools = |source: &Settable| {
        let spec = crate::addons::Spec {
            tiles: "/opt/tiles".into(),
            draw: tiles_mcp::instructions::Draw::Image,
            companion: false,
            reach: crate::serve::reach(&[source]),
            env: Vec::new(),
            forms: Vec::new(),
        };
        crate::addons::pi_tool(&spec)["tools"].clone()
    };
    assert_eq!(tools(&source), serde_json::json!([]));
    source.command(&args("setup")).unwrap();
    assert_eq!(
        tools(&source),
        serde_json::json!([{ "name": "tiles_warehouse_models", "description": "d", "inputSchema": {} }])
    );
}

#[test]
fn licence_prints_the_code_design_and_font_licences() {
    assert_eq!(parse_args(&args("--licence")), Ok(Command::Licence));
    assert!(parse_args(&args("--licence now")).is_err());
    let text = super::licence_lines();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].starts_with("Code: FSL-1.1-ALv2"), "{text}");
    assert!(lines[2].contains("SIL Open Font License 1.1"), "{text}");
    assert!(!text.contains("placeholder"), "{text}");
    let pack = tiles_engine::SEAL_SOURCE.strip_prefix("pack ");
    assert_eq!(
        lines[1].starts_with("Design pack "),
        pack.is_some(),
        "{text}"
    );
    // Only the house pack is under the design licence; the plain theme a
    // build from source draws is part of the code (ADR-040).
    assert_eq!(
        lines[1].contains("the Tiles Design Licence"),
        pack.is_some(),
        "{text}"
    );
}
