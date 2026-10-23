//! `tiles doctor`: the terminal, the image route and why, the cell size cards
//! are drawn for and where it came from, each harness and its add-on, the card
//! cache, and the Omni configuration, with no secret in any line (ADR-008,
//! ADR-009). The Omni key is reported as set or not; the URL
//! only by its host. With Omni or a registered source configured, an install
//! that still runs a Tiles tool without asking is flagged with the command
//! that fixes it; each registered source adds its own lines after these. Each
//! harness's row names its version and the newest one Tiles was proved on,
//! and the update check's setting and last run have a line (ADR-032). After the
//! harness rows, a `setup:` line names the harnesses `tiles setup` would set
//! up (ADR-035). On the text route a `pictures:` line says cards will not draw
//! inline there and how they open instead (ADR-046).

use std::path::PathBuf;

use tiles_omni::{OmniConfig, SecretStore};
use tiles_term::detect::{self, Replies};

use crate::harness::Harness;
use crate::install::Dirs;

/// What the report is built from.
pub struct Inputs<'a> {
    /// One environment variable.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The terminal's answers, when it was asked.
    pub replies: Option<&'a Replies>,
    /// Why it was not asked: neither standard input nor output is a
    /// terminal, or one is not and `/dev/tty` is no terminal either.
    pub not_asked: Option<&'a str>,
    /// One cell in pixels from the window size, when the terminal sets it.
    pub window: Option<(u16, u16)>,
    /// Where a program is on `PATH`.
    pub which: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// The Keychain, or a fake.
    pub secrets: &'a dyn SecretStore,
    /// The cache directory, or why there is none.
    pub cache: Result<PathBuf, String>,
    /// Where the harnesses keep their config, when `HOME` is set.
    pub dirs: Option<Dirs>,
    /// What `opencode --help` and `--version` said, when it is on `PATH`.
    pub opencode: Option<&'a crate::opencode::Found>,
    /// macOS's Reduce Motion, `None` when it cannot be read.
    pub reduce_motion: Option<bool>,
    /// Each harness on `PATH` that was asked its `--version`, with the
    /// version it named.
    pub versions: &'a [(Harness, Option<String>)],
    /// The daily update check's state, `None` when it is off or has
    /// nowhere to keep it (ADR-032).
    pub update: Option<&'a crate::update::State>,
    /// The tools of each registered source that is configured, which an
    /// install must ask before running.
    pub data_tools: Vec<&'static str>,
    /// The seconds Codex must wait for a Tiles tool call, when a configured
    /// source's chart call may take longer than Codex's own default.
    pub codex_timeout: Option<u64>,
    /// The hosts each registered source reaches, in words.
    pub network: Vec<&'static str>,
    /// `tiles setup`'s hint, one line each ([`crate::setup::hints`]).
    pub setup: Vec<String>,
    /// One line per other `tiles` ahead of this one on `PATH`, with the
    /// command that moves it aside ([`crate::copies::Copies::doctor`]).
    pub copies: Vec<String>,
}

fn installed(h: Harness, dirs: Option<&Dirs>) -> &'static str {
    let exists = |p: Option<PathBuf>| p.is_some_and(|p| p.exists());
    match h {
        Harness::Pi => {
            if exists(dirs.map(|d| d.home.join(".pi/agent/extensions/tiles/index.ts"))) {
                "add-on installed"
            } else {
                "add-on not installed (tiles run pi loads it per session)"
            }
        }
        Harness::OpenCode => {
            if exists(dirs.map(|d| d.config_home.join("opencode/tiles-instructions.md"))) {
                "installed (config.json)"
            } else {
                "not installed (tiles run opencode loads Tiles per session)"
            }
        }
        Harness::Claude | Harness::Codex => {
            "MCP server per session through tiles run; tiles install adds it for good"
        }
    }
}

/// The terminal, its answers and the cell size cards are drawn for.
fn terminal_lines(i: &Inputs<'_>) -> Vec<String> {
    let clean = |s: &str| tiles_term::sanitize::line(s, 200);
    let mut out = Vec::new();
    let mut cards = None;
    let terminal = match detect::detect(i.env, i.replies) {
        Ok(d) => {
            out.push(format!("terminal: {d}"));
            cards = crate::companion::text_route_line(&d, i.env);
            d.terminal
        }
        Err(e) => {
            out.push(format!("terminal: {e}"));
            detect::Terminal::Unknown
        }
    };
    if let Some(r) = i.replies {
        let mut answers = Vec::new();
        if let Some(v) = &r.xtversion {
            answers.push(format!("XTVERSION {v}"));
        }
        answers.push(format!(
            "kitty graphics {}",
            if r.kitty_graphics { "yes" } else { "no" }
        ));
        answers.push(format!("sixel {}", if r.sixel { "yes" } else { "no" }));
        if let Some((w, h)) = r.cell_px {
            answers.push(format!("cell {w}x{h} px"));
        }
        if let Some(light) = r.light_background() {
            answers.push(format!(
                "{} background",
                if light { "light" } else { "dark" }
            ));
        }
        out.push(format!("  answers: {}", answers.join(", ")));
    } else {
        out.push(format!(
            "  answers: not asked ({})",
            clean(i.not_asked.unwrap_or("not a terminal"))
        ));
    }
    out.push(
        match crate::run::cell(i.replies, i.env, terminal, i.window) {
            Some((c, from)) => format!("  cell size: {} device px, from {from}", clean(&c)),
            None => "  cell size: not reported; cards use 16x32 device px".into(),
        },
    );
    out.extend(cards.map(|c| format!("  {c}")));
    out
}

/// The `harnesses:` line and one row per harness, with its version and
/// install state.
fn harness_lines(i: &Inputs<'_>) -> Vec<String> {
    let clean = |s: &str| tiles_term::sanitize::line(s, 200);
    let mut out = vec!["harnesses:".to_string()];
    for h in Harness::ALL {
        match (i.which)(h.name()) {
            Some(path) => {
                let version = i.versions.iter().find(|(v, _)| *v == h).map(|(_, v)| {
                    let cli = i.opencode.and_then(|f| f.cli);
                    format!(
                        "{}  ",
                        clean(&crate::proved::doctor_note(h, cli, v.as_deref()))
                    )
                });
                out.push(format!(
                    "  {:<9}{}  {}{}",
                    h.name(),
                    clean(&path.display().to_string()),
                    version.unwrap_or_default(),
                    installed(h, i.dirs.as_ref())
                ));
                if let (Harness::OpenCode, Some(found)) = (h, i.opencode) {
                    out.push(format!(
                        "{:11}{}",
                        "",
                        tiles_term::sanitize::line(&found.describe().1, 600)
                    ));
                }
            }
            None => out.push(format!("  {:<9}not found on PATH", h.name())),
        }
    }
    out.extend(i.setup.iter().cloned());
    out
}

/// The report, line by line.
#[must_use]
pub fn report(i: &Inputs<'_>) -> Vec<String> {
    let clean = |s: &str| tiles_term::sanitize::line(s, 200);
    let mut out = vec![format!(
        "tiles {} (intent schema {})",
        tiles_intent::TILES_VERSION,
        tiles_intent::SCHEMA_VERSION
    )];
    out.extend(i.copies.iter().map(|l| {
        format!(
            "  {}",
            tiles_term::sanitize::line(l, crate::copies::LONGEST)
        )
    }));
    let current = tiles_intent::TILES_VERSION;
    out.extend(i.update.and_then(|state| {
        crate::update::notice(current, state, crate::update::packaged_here(i.env))
    }));
    out.extend(terminal_lines(i));
    out.push(format!(
        "  overrides: TILES_TERMINAL={}, TILES_ROUTE={}, TILES_CELL_PX={}",
        clean(&(i.env)("TILES_TERMINAL").unwrap_or_else(|| "unset".into())),
        clean(&(i.env)("TILES_ROUTE").unwrap_or_else(|| "unset".into())),
        clean(&(i.env)("TILES_CELL_PX").unwrap_or_else(|| "unset".into())),
    ));
    let (motion, source) = crate::motion::setting(i.env, || i.reduce_motion);
    out.push(format!(
        "motion: {}, from {} (TILES_MOTION=on|reduce|off)",
        motion.name(),
        source.name()
    ));
    out.extend(harness_lines(i));
    match &i.cache {
        Ok(dir) => {
            let cards = std::fs::read_dir(dir.join("cards")).map_or(0, |d| {
                d.filter_map(Result::ok)
                    .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
                    .count()
            });
            out.push(format!(
                "cards: {} ({cards} saved)",
                clean(&dir.display().to_string())
            ));
        }
        Err(e) => out.push(format!("cards: {}", clean(e))),
    }
    let mut data_tools = i.data_tools.clone();
    match OmniConfig::resolve(i.env, i.secrets) {
        Ok(cfg) => {
            data_tools.extend([tiles_mcp::OMNI_TOPICS_TOOL, tiles_mcp::OMNI_FIELDS_TOOL]);
            out.push(format!(
                "omni: configured for {}, API key set {}, {}",
                clean(cfg.base.authority()),
                match cfg.source {
                    tiles_omni::Source::Environment => "in the environment",
                    tiles_omni::Source::Keychain => "in the Keychain",
                },
                cfg.model.as_ref().map_or_else(
                    || "every shared model".to_string(),
                    |m| format!("model {}", m.as_str())
                )
            ));
        }
        Err(e) => out.push(format!("omni: not configured: {}", clean(&e.to_string()))),
    }
    if !data_tools.is_empty() {
        for stale in i
            .dirs
            .as_ref()
            .map(|d| crate::install::stale_installs(d, &data_tools, i.codex_timeout))
            .unwrap_or_default()
        {
            out.push(format!("  warning: {}", clean(&stale)));
        }
    }
    out.push(clean(&crate::update::doctor_line(
        i.env,
        current,
        i.update,
        crate::update::now(),
    )));
    let mut network =
        "network: none, except Omni queries to the host above (ADR-005), the update check (ADR-032)"
            .to_string();
    for hosts in &i.network {
        network.push_str(", ");
        network.push_str(&clean(hosts));
    }
    if let Some(at) = network.rfind(", ") {
        network.replace_range(at..at + 2, " and ");
    }
    out.push(network);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use tiles_omni::{ConfigError, SecretStore};
    use tiles_term::detect::Replies;

    use super::{Inputs, report};
    use crate::install::Dirs;

    struct Store(Option<String>);

    impl SecretStore for Store {
        fn get(&self, account: &str) -> Result<Option<String>, ConfigError> {
            Ok(match account {
                "url" => self
                    .0
                    .as_ref()
                    .map(|_| "https://acme.omniapp.co".to_string()),
                "api-key@acme.omniapp.co" => self.0.clone(),
                _ => None,
            })
        }
    }

    const KEY: &str = "omni_planted_key_7Yq2xL9vR4";

    fn run(vars: &[(&str, &str)], store: &Store) -> String {
        run_in(vars, store, None)
    }

    fn run_in(vars: &[(&str, &str)], store: &Store, dirs: Option<Dirs>) -> String {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        let env = move |k: &str| map.get(k).cloned();
        let which = |p: &str| (p == "pi").then(|| PathBuf::from("/usr/local/bin/pi"));
        let replies = Replies::parse(b"\x1b_Gi=31;OK\x1b\\\x1bP>|WarpTerminal\x1b\\\x1b[?62c");
        report(&Inputs {
            env: &env,
            replies: Some(&replies),
            not_asked: None,
            window: Some((8, 16)),
            which: &which,
            secrets: store,
            cache: Ok(PathBuf::from("/c")),
            dirs,
            opencode: None,
            reduce_motion: Some(true),
            versions: &[],
            update: None,
            data_tools: Vec::new(),
            codex_timeout: None,
            network: Vec::new(),
            setup: Vec::new(),
            copies: Vec::new(),
        })
        .join("\n")
    }

    #[test]
    fn opencode_is_reported_with_its_version_and_server() {
        let env = |_: &str| None;
        let which = |p: &str| (p == "opencode").then(|| PathBuf::from("/u/.local/bin/opencode"));
        let text = |version: Option<&str>| {
            let found = crate::opencode::Found {
                cli: Some(crate::opencode::Cli::Launcher),
                version: version.map(str::to_string),
            };
            report(&Inputs {
                env: &env,
                replies: None,
                not_asked: Some("not a terminal"),
                window: None,
                which: &which,
                secrets: &Store(None),
                cache: Ok(PathBuf::from("/c")),
                dirs: None,
                opencode: Some(&found),
                reduce_motion: None,
                versions: &[],
                update: None,
                data_tools: Vec::new(),
                codex_timeout: None,
                network: Vec::new(),
                setup: Vec::new(),
                copies: Vec::new(),
            })
            .join("\n")
        };
        let said = text(Some("2.0.32"));
        assert!(
            said.contains(
                "  opencode /u/.local/bin/opencode  not installed (tiles run opencode loads Tiles per session)\n           OpenCode 2.0.32, a launcher with a background service: tiles run passes --standalone"
            ),
            "{said}"
        );
        let said = text(None);
        let sentence = crate::opencode::Found {
            cli: Some(crate::opencode::Cli::Launcher),
            version: None,
        }
        .describe()
        .1;
        assert!(sentence.len() > 200, "{sentence}");
        assert!(
            said.contains(&format!("\n           {sentence}\n")),
            "the whole sentence, uncut:\n{said}"
        );
    }

    /// ADR-032: each harness's row names its version and the one Tiles was
    /// proved on; the update notice follows the version line and the
    /// check's setting comes before the network line.
    #[test]
    fn the_rows_name_each_version_and_the_update_check_has_a_line() {
        use crate::harness::Harness;
        let env = |_: &str| None;
        let which = |p: &str| {
            matches!(p, "pi" | "codex").then(|| PathBuf::from(format!("/usr/local/bin/{p}")))
        };
        let state = crate::update::State {
            checked: Some(1_792_436_886),
            latest: Some(crate::update::Version::parse("0.1.5").unwrap()),
        };
        let versions = [
            (Harness::Pi, Some("1.4.0".to_string())),
            (Harness::Codex, None),
        ];
        let text = report(&Inputs {
            env: &env,
            replies: None,
            not_asked: Some("not a terminal"),
            window: None,
            which: &which,
            secrets: &Store(None),
            cache: Ok(PathBuf::from("/c")),
            dirs: None,
            opencode: None,
            reduce_motion: None,
            versions: &versions,
            update: Some(&state),
            data_tools: Vec::new(),
            codex_timeout: None,
            network: Vec::new(),
            setup: Vec::new(),
            copies: Vec::new(),
        });
        let joined = text.join("\n");
        assert!(
            joined.contains(
                "\n  pi       /usr/local/bin/pi  1.4.0 (proved on 1.4.0)  add-on not installed"
            ),
            "{joined}"
        );
        assert!(
            joined.contains(
                "\n  codex    /usr/local/bin/codex  version unknown (proved on 0.163.0)  MCP server"
            ),
            "{joined}"
        );
        assert_eq!(text[1], "Tiles v0.1.5 is available; run tiles update");
        let network = text
            .iter()
            .position(|l| l.starts_with("network: "))
            .unwrap();
        assert!(text[network - 1].starts_with("update check: "), "{joined}");
        assert_eq!(
            text[network],
            "network: none, except Omni queries to the host above (ADR-005) and the update check (ADR-032)"
        );
    }

    #[test]
    fn the_report_names_the_motion_setting_and_its_source() {
        let off = |k: &str| (k == "TILES_MOTION").then(|| "off".to_string());
        let which = |_: &str| None;
        let text = |env: &dyn Fn(&str) -> Option<String>, system: Option<bool>| {
            report(&Inputs {
                env,
                replies: None,
                not_asked: Some("not a terminal"),
                window: None,
                which: &which,
                secrets: &Store(None),
                cache: Ok(PathBuf::from("/c")),
                dirs: None,
                opencode: None,
                reduce_motion: system,
                versions: &[],
                update: None,
                data_tools: Vec::new(),
                codex_timeout: None,
                network: Vec::new(),
                setup: Vec::new(),
                copies: Vec::new(),
            })
            .join("\n")
        };
        assert!(
            text(&off, Some(true))
                .contains("\nmotion: off, from TILES_MOTION (TILES_MOTION=on|reduce|off)\n")
        );
        let none = |_: &str| None;
        assert!(text(&none, Some(true)).contains("\nmotion: reduce, from macOS Reduce Motion "));
        assert!(text(&none, None).contains("\nmotion: on, from default "));
    }

    #[test]
    fn the_report_names_terminal_route_harnesses_and_omni() {
        let text = run(
            &[
                ("TERM_PROGRAM", "WarpTerminal"),
                ("TILES_OMNI_URL", "https://acme.omniapp.co"),
                ("TILES_OMNI_API_KEY", KEY),
            ],
            &Store(None),
        );
        assert!(
            text.contains("terminal: warp, route kitty-precut"),
            "{text}"
        );
        assert!(text.contains("kitty graphics yes"));
        assert!(
            text.contains("  cell size: 16x32 device px, from the window size, read as points"),
            "{text}"
        );
        assert!(text.contains("  pi       /usr/local/bin/pi"));
        assert!(text.contains("  codex    not found on PATH"));
        assert!(
            text.contains("omni: configured for acme.omniapp.co, API key set in the environment")
        );
        let text = run(&[], &Store(Some(KEY.into())));
        assert!(text.contains("omni: configured for acme.omniapp.co, API key set in the Keychain"));
    }

    #[test]
    fn on_the_text_route_a_line_says_cards_will_not_draw_inline() {
        let text = run(&[("TERM_PROGRAM", "Apple_Terminal")], &Store(None));
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("terminal: apple-terminal, route text"))
            .unwrap_or_else(|| panic!("{text}"));
        let pictures = lines[at + 1..]
            .iter()
            .find(|l| l.starts_with("  pictures: "))
            .unwrap_or_else(|| panic!("{text}"));
        assert!(
            pictures.starts_with("  pictures: none in this terminal, so cards will not draw inline here; Warp, Ghostty, kitty, iTerm2 and WezTerm draw them inline. Here "),
            "{pictures}"
        );
        assert_eq!(text.matches("  pictures: ").count(), 1, "{text}");
        let warp = run(&[("TERM_PROGRAM", "WarpTerminal")], &Store(None));
        assert!(!warp.contains("  pictures: "), "{warp}");
    }

    #[test]
    fn no_secret_appears_whether_the_key_comes_from_env_or_keychain() {
        for (vars, store) in [
            (
                vec![
                    ("TILES_OMNI_URL", "https://acme.omniapp.co"),
                    ("TILES_OMNI_API_KEY", KEY),
                ],
                Store(None),
            ),
            (
                vec![("TILES_OMNI_URL", "https://acme.omniapp.co")],
                Store(Some(KEY.into())),
            ),
            (vec![], Store(Some(KEY.into()))),
            (
                vec![("TILES_OMNI_URL", "ftp://bad"), ("TILES_OMNI_API_KEY", KEY)],
                Store(None),
            ),
            (vec![("TILES_OMNI_API_KEY", KEY)], Store(None)),
        ] {
            let text = run(&vars, &store);
            assert!(!text.contains(KEY), "{text}");
            assert!(!text.contains("7Yq2"), "{text}");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One machine, checked with Omni, without it and with the warehouse.
    fn an_install_that_still_runs_tiles_tools_unasked_is_flagged_once_omni_is_set() {
        let root = std::env::temp_dir().join(format!("tiles-doctor-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs {
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
            &root.join("codex/config.toml"),
            b"[mcp_servers.tiles]\ncommand = \"/opt/tiles\"\nargs = [\"mcp\"]\n",
        )
        .unwrap();
        let keychain = Store(Some(KEY.into()));
        let text = run_in(&[], &keychain, Some(dirs.clone()));
        let warnings: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("  warning: "))
            .collect();
        assert_eq!(warnings.len(), 2, "{text}");
        assert!(
            warnings[0].ends_with("run tiles install opencode"),
            "{text}"
        );
        assert!(warnings[1].ends_with("run tiles install codex"), "{text}");
        let no_omni = run_in(&[], &Store(None), Some(dirs.clone()));
        assert!(!no_omni.contains("warning:"), "{no_omni}");
        let warehouse = |dirs: Option<Dirs>| {
            report(&Inputs {
                env: &|_| None,
                replies: None,
                not_asked: Some("not a terminal"),
                window: None,
                which: &|_| None,
                secrets: &Store(None),
                cache: Ok(PathBuf::from("/c")),
                dirs,
                opencode: None,
                reduce_motion: None,
                versions: &[],
                update: None,
                data_tools: vec!["tiles_warehouse_models", "tiles_warehouse_columns"],
                codex_timeout: Some(330),
                network: vec!["warehouse calls to bigquery.googleapis.com and oauth2.googleapis.com (ADR-031)"],
                setup: Vec::new(),
                copies: Vec::new(),
            })
            .join("\n")
        };
        let said = warehouse(Some(dirs.clone()));
        assert_eq!(
            said.lines()
                .filter(|l| l.starts_with("  warning: "))
                .count(),
            3,
            "a configured warehouse flags them too, and Codex's short wait: {said}"
        );
        assert!(
            said.contains("  warning: Codex's tiles server has no tool_timeout_sec, so Codex may stop waiting before a query ends; run tiles install codex"),
            "{said}"
        );
        assert!(
            said.ends_with(
                "network: none, except Omni queries to the host above (ADR-005), the update check (ADR-032) and warehouse calls to bigquery.googleapis.com and oauth2.googleapis.com (ADR-031)"
            ),
            "{said}"
        );
        let changed = crate::install::reapply_approvals(
            &dirs,
            &[tiles_mcp::OMNI_TOPICS_TOOL, tiles_mcp::OMNI_FIELDS_TOOL],
            None,
        );
        assert_eq!(changed.len(), 2, "{changed:?}");
        let text = run_in(&[], &keychain, Some(dirs.clone()));
        assert!(!text.contains("warning:"), "{text}");
        let said = warehouse(Some(dirs.clone()));
        assert_eq!(
            said.lines()
                .filter(|l| l.starts_with("  warning: "))
                .count(),
            2,
            "the OpenCode file does not list the warehouse tools yet, and Codex waits too little: {said}"
        );
        crate::install::reapply_approvals(
            &dirs,
            &["tiles_warehouse_models", "tiles_warehouse_columns"],
            Some(330),
        );
        assert!(!warehouse(Some(dirs)).contains("warning:"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hostile_environment_values_are_printed_plain() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for e in json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
        {
            for key in [
                "TILES_CELL_PX",
                "TILES_TERMINAL",
                "TILES_ROUTE",
                "TILES_OMNI_URL",
            ] {
                let text = run(&[(key, e)], &Store(None));
                assert!(
                    text.lines()
                        .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char)),
                    "{key} {e:?}"
                );
            }
        }
        assert!(run(&[], &Store(None)).contains("omni: not configured: no Omni URL"));
    }

    #[test]
    fn a_harness_on_path_without_tiles_set_up_is_offered_tiles_setup() {
        use crate::harness::Harness;
        use crate::setup::{Plan, hints};
        let lines = |plans: &[(Harness, Plan)]| {
            report(&Inputs {
                env: &|_| None,
                replies: None,
                not_asked: Some("not a terminal"),
                window: None,
                which: &|_| None,
                secrets: &Store(None),
                cache: Ok(PathBuf::from("/c")),
                dirs: None,
                opencode: None,
                reduce_motion: None,
                versions: &[],
                update: None,
                data_tools: Vec::new(),
                codex_timeout: None,
                network: Vec::new(),
                setup: hints(plans),
                copies: Vec::new(),
            })
        };
        let install = |h: &str| Plan::Install(PathBuf::from(h), Vec::new());
        let text = lines(&[
            (Harness::Pi, install("/bin/pi")),
            (
                Harness::Claude,
                Plan::Replace(
                    PathBuf::from("/bin/claude"),
                    "set up with another tiles, /old/tiles".into(),
                    Vec::new(),
                ),
            ),
            (Harness::Codex, install("/bin/codex")),
        ]);
        let rows = text
            .iter()
            .rposition(|l| l.ends_with("not found on PATH"))
            .unwrap();
        assert_eq!(
            text[rows + 1..rows + 3],
            [
                "  setup: Tiles is not set up, or is out of date, for Pi and Codex; run tiles setup",
                "  setup: Claude Code is set up with another tiles, /old/tiles; run tiles setup to replace it",
            ]
        );
        let current = lines(&[(
            Harness::OpenCode,
            Plan::Current(PathBuf::from("/bin/opencode")),
        )]);
        assert!(!current.iter().any(|l| l.contains("tiles setup")));
    }
}
