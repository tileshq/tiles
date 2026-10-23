use std::cell::RefCell;
use std::path::{Path, PathBuf};

use tiles_mcp::source::Found;
use tiles_omni::{ConfigError, SecretStore};

use super::{
    Args, Inputs, Plan, Registered, Report, add_cards, apply, hints, omni, parse, plan, report, run,
};
use crate::copies::{Copies, Other, Place};
use crate::harness::Harness;
use crate::install::{self, Dirs, Step};

fn dirs(name: &str) -> Dirs {
    let root = std::env::temp_dir().join(format!("tiles-setup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    Dirs {
        home: root.join("home"),
        config_home: root.join("xdg"),
        codex_home: root.join("codex"),
    }
}

fn spec(h: Harness) -> crate::addons::Spec {
    install::spec(
        h,
        PathBuf::from("/opt/tiles"),
        tiles_mcp::source::Reach::default(),
        Vec::new(),
    )
}

fn on_path(name: &str) -> PathBuf {
    PathBuf::from(format!("/bin/{name}"))
}

/// What `codex mcp add tiles --env TILES_DRAW=text -- /opt/tiles mcp`
/// (codex-cli 0.163.0) writes.
const CODEX_ADDED: &str = "[mcp_servers.tiles]\ncommand = \"/opt/tiles\"\nargs = [\"mcp\"]\n\n[mcp_servers.tiles.env]\nTILES_DRAW = \"text\"\n";

/// What `claude mcp get tiles` (Claude Code 2.1.304) prints after `claude
/// mcp add --scope user tiles -e TILES_DRAW=text -- /opt/tiles mcp`.
const CLAUDE_ADDED: &str = "tiles:\n  Scope: User config (available in all your projects)\n  Status: \u{2714} Connected\n  Type: stdio\n  Command: /opt/tiles\n  Args: mcp\n  Environment:\n    TILES_DRAW=text\n\nTo remove this server, run: claude mcp remove \"tiles\" -s user\n";

#[test]
fn it_takes_yes_and_dry_run_only() {
    let args = |a: &[&str]| parse(&a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
    assert_eq!(args(&[]).unwrap(), Args::default());
    assert_eq!(
        args(&["--yes", "--dry-run"]).unwrap(),
        Args {
            yes: true,
            dry_run: true
        }
    );
    assert!(args(&["-y"]).is_err(), "--yes is the one spelling");
    let e = args(&["--force\u{1b}[2J"]).unwrap_err();
    assert!(e.contains("usage: tiles setup"), "{e}");
    assert!(!e.contains('\u{1b}'), "{e:?}");
}

#[test]
fn every_harness_on_path_is_planned_and_an_install_that_matches_is_current() {
    let dirs = dirs("plan");
    let claude_has = RefCell::new(None);
    let has = |_: &Path| claude_has.borrow().as_deref().map(Registered::printed);
    let inputs = Inputs {
        which: &|name| (name != "opencode").then(|| on_path(name)),
        dirs: &dirs,
        spec: &spec,
        claude_tiles: &has,
    };
    assert_eq!(plan(Harness::OpenCode, &inputs), Plan::Missing);
    for h in [Harness::Pi, Harness::Claude, Harness::Codex] {
        let Plan::Install(path, steps) = plan(h, &inputs) else {
            panic!("{h:?} should be planned");
        };
        assert_eq!(path, PathBuf::from(format!("/bin/{}", h.name())));
        assert_ne!(steps, []);
    }
    // Carry out what tiles install does, with the harness commands faked
    // by what they write.
    let codex_file = dirs.codex_home.join("config.toml");
    let run = |program: &str, _: &[String]| {
        if program == "codex" {
            crate::addons::write_file(&codex_file, CODEX_ADDED.as_bytes())
        } else {
            *claude_has.borrow_mut() = Some(CLAUDE_ADDED.to_string());
            Ok(())
        }
    };
    for h in [Harness::Pi, Harness::Claude, Harness::Codex] {
        let Plan::Install(_, steps) = plan(h, &inputs) else {
            panic!()
        };
        install::apply(&steps, &run).unwrap();
        assert_eq!(
            plan(h, &inputs),
            Plan::Current(PathBuf::from(format!("/bin/{}", h.name()))),
            "{h:?} is set up after its install"
        );
    }
    // An install for another tiles binary, or an edited add-on, is planned
    // again.
    std::fs::write(&codex_file, CODEX_ADDED.replace("/opt/tiles", "/old/tiles")).unwrap();
    assert!(matches!(plan(Harness::Codex, &inputs), Plan::Install(..)));
    let tool = install::pi_extension(&dirs.home).join("tool.json");
    std::fs::write(&tool, "{}").unwrap();
    assert!(matches!(plan(Harness::Pi, &inputs), Plan::Install(..)));
    let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
}

#[test]
fn a_codex_install_with_a_warehouse_waits_out_its_query_and_a_second_run_changes_nothing() {
    let dirs = dirs("codex-timeout");
    let warehouse = |h: Harness| {
        let mut reach = tiles_mcp::source::Reach::default();
        reach.offers.push(tiles_mcp::source::Offer {
            name: "warehouse",
            origin: "BigQuery",
            tools: Vec::new(),
            instruction: String::new(),
            chart_line: String::new(),
            tool_timeout: std::time::Duration::from_secs(300),
        });
        install::spec(h, PathBuf::from("/opt/tiles"), reach, Vec::new())
    };
    let inputs = Inputs {
        which: &|name| (name == "codex").then(|| on_path(name)),
        dirs: &dirs,
        spec: &warehouse,
        claude_tiles: &|_| None,
    };
    let Plan::Install(_, steps) = plan(Harness::Codex, &inputs) else {
        panic!("Codex should be planned");
    };
    let codex_file = dirs.codex_home.join("config.toml");
    assert!(
        steps.contains(&Step::CodexTimeout(codex_file.clone(), 330)),
        "{steps:?}"
    );
    let run =
        |_: &str, _: &[String]| crate::addons::write_file(&codex_file, CODEX_ADDED.as_bytes());
    install::apply(&steps, &run).unwrap();
    let config: toml::Table = std::fs::read_to_string(&codex_file)
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        config["mcp_servers"]["tiles"]["tool_timeout_sec"].as_integer(),
        Some(330)
    );
    assert_eq!(
        plan(Harness::Codex, &inputs),
        Plan::Current(on_path("codex")),
        "a second run changes nothing"
    );
    let longer = std::fs::read_to_string(&codex_file)
        .unwrap()
        .replace("tool_timeout_sec = 330", "tool_timeout_sec = 900");
    std::fs::write(&codex_file, &longer).unwrap();
    assert_eq!(
        plan(Harness::Codex, &inputs),
        Plan::Current(on_path("codex")),
        "a longer wait of the user's is enough"
    );
    let short = longer.replace("= 900", "= 60");
    std::fs::write(&codex_file, &short).unwrap();
    assert!(matches!(plan(Harness::Codex, &inputs), Plan::Install(..)));
    let warned = |config: &str| {
        std::fs::write(&codex_file, config).unwrap();
        install::stale_installs(&dirs, &[], Some(330))
    };
    assert_eq!(
        warned(&short),
        [
            "Codex's tiles server has a tool_timeout_sec under 330, so Codex may stop waiting before a query ends; run tiles install codex"
        ]
    );
    assert_eq!(
        warned(&short.replace("tool_timeout_sec = 60\n", "")),
        [
            "Codex's tiles server has no tool_timeout_sec, so Codex may stop waiting before a query ends; run tiles install codex"
        ]
    );
    let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
}

/// The steps that replace Claude Code's `tiles` server with `/opt/tiles`'s.
fn claude_replace() -> Vec<Step> {
    let mut steps = vec![Step::Run(
        "claude".into(),
        ["mcp", "remove", "tiles", "--scope", "user"]
            .map(String::from)
            .to_vec(),
    )];
    steps.extend(install::steps(
        Harness::Claude,
        Path::new("/h"),
        Path::new("/h"),
        Path::new("/h"),
        &spec(Harness::Claude),
    ));
    steps
}

#[test]
fn a_user_scope_claude_server_for_another_tiles_is_replaced_and_any_other_is_left() {
    let dirs = dirs("claude-other");
    let other = |answer: String| {
        let has = move |_: &Path| Some(Registered::printed(&answer));
        let inputs = Inputs {
            which: &|name| Some(on_path(name)),
            dirs: &dirs,
            spec: &spec,
            claude_tiles: &has,
        };
        plan(Harness::Claude, &inputs)
    };
    let claude = on_path("claude");
    let elsewhere = CLAUDE_ADDED.replace("/opt/tiles", "/Users/me/.local/bin/tiles");
    assert_eq!(
        other(elsewhere.clone()),
        Plan::Replace(
            claude.clone(),
            "set up with another tiles, /Users/me/.local/bin/tiles".into(),
            claude_replace()
        )
    );
    assert!(
        matches!(
            other(elsewhere.replace("TILES_DRAW=text", "TILES_DRAW=image")),
            Plan::Replace(..)
        ),
        "the draw variable is the one tiles install writes back"
    );
    let fix = "run claude mcp remove tiles --scope user, then tiles setup";
    assert_eq!(
        other(elsewhere.replace("    TILES_DRAW=text\n", "    TILES_DRAW=text\n    MINE=1\n")),
        Plan::Other(
            claude.clone(),
            format!(
                "set up with another tiles, /Users/me/.local/bin/tiles, with variables of yours; {fix}"
            )
        ),
        "a variable of the user's is never removed"
    );
    for (scope, label) in [
        ("project", "Project config (shared via .mcp.json)"),
        ("local", "Local config (private to you in this project)"),
    ] {
        assert_eq!(
            other(elsewhere.replace("User config (available in all your projects)", label)),
            Plan::Other(
                claude.clone(),
                format!(
                    "set up with another tiles, /Users/me/.local/bin/tiles, in Claude Code's {scope} config; Tiles leaves it: run claude mcp remove tiles --scope {scope}, then tiles setup"
                )
            ),
            "{scope}"
        );
    }
    assert_eq!(
        other(elsewhere.replace(
            "  Scope: User config (available in all your projects)\n",
            ""
        )),
        Plan::Other(
            claude.clone(),
            format!("set up with another tiles, /Users/me/.local/bin/tiles; {fix}")
        ),
        "a server whose scope is not said is left"
    );
    let without_env = CLAUDE_ADDED.replace("  Environment:\n    TILES_DRAW=text\n", "");
    let with_env = CLAUDE_ADDED.replace("TILES_DRAW=text", "TILES_DRAW=image");
    for answer in [
        without_env,
        with_env,
        CLAUDE_ADDED.replace("Args: mcp", "Args: "),
    ] {
        assert_eq!(
            other(answer),
            Plan::Other(
                claude.clone(),
                format!("set up with this tiles, but not with mcp and TILES_DRAW=text; {fix}")
            )
        );
    }
    let mine = CLAUDE_ADDED.replace("    TILES_DRAW=text\n", "    TILES_DRAW=text\n    MINE=1\n");
    assert_eq!(
        other(mine),
        Plan::Current(claude.clone()),
        "a variable of the user's is kept"
    );
    assert!(matches!(
        other("tiles:\n  Scope: User config (available in all your projects)\n  Type: http\n  URL: https://x\n".into()),
        Plan::Other(..)
    ));
    let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
}

#[test]
fn a_replacement_is_listed_before_the_one_question() {
    let replace = Plan::Replace(
        on_path("claude"),
        "set up with another tiles, /old/tiles".into(),
        claude_replace(),
    );
    let r = report(
        vec![(Harness::Claude, replace.clone())],
        "terminal: Warp",
        &Copies::default(),
        &[],
    );
    assert_eq!(
        r.found[1],
        "claude: /bin/claude, set up with another tiles, /old/tiles; Tiles will replace it"
    );
    assert_eq!(r.installs, [(Harness::Claude, claude_replace())]);
    assert!(!r.left);
    let (text, installed, ok) = drive(Args::default(), &r, None);
    assert!(installed.is_empty() && ok, "no answer replaces nothing");
    assert!(
        text.contains(
            "Tiles will change:\n  claude: run claude mcp remove tiles --scope user\n  claude: run claude mcp add --scope user tiles -e TILES_DRAW=text -- /opt/tiles mcp\nSet up Tiles for Claude Code? [y/N] "
        ),
        "{text}"
    );
    assert!(text.contains("Nothing changed."), "{text}");
    assert_eq!(
        hints(&[(Harness::Claude, replace)]),
        [
            "  setup: Claude Code is set up with another tiles, /old/tiles; run tiles setup to replace it"
        ]
    );
}

/// A Claude Code faked by what its `mcp` commands do to its `tiles`
/// server, which starts as `has`; `remove` fails when `fails`.
struct FakeClaude {
    has: RefCell<Option<String>>,
    fails: bool,
    ran: RefCell<Vec<String>>,
}

impl FakeClaude {
    fn new(has: &str, fails: bool) -> Self {
        Self {
            has: RefCell::new(Some(has.to_string())),
            fails,
            ran: RefCell::new(Vec::new()),
        }
    }

    fn run(&self, program: &str, args: &[String]) -> Result<(), String> {
        self.ran
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        match args.get(1).map(String::as_str) {
            Some("remove") if self.fails => Err("claude exited with exit status: 1".into()),
            Some("remove") => {
                *self.has.borrow_mut() = None;
                Ok(())
            }
            Some("add") if self.has.borrow().is_some() => {
                Err("claude exited with exit status: 1".into())
            }
            Some("add") => {
                *self.has.borrow_mut() = Some(CLAUDE_ADDED.to_string());
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[test]
fn a_stale_claude_server_is_replaced_and_a_current_one_is_untouched() {
    let dirs = dirs("claude-replace");
    let stale = CLAUDE_ADDED.replace("/opt/tiles", "/old/tiles");
    let fake = FakeClaude::new(&stale, false);
    let has = |_: &Path| fake.has.borrow().as_deref().map(Registered::printed);
    let inputs = Inputs {
        which: &|name| Some(on_path(name)),
        dirs: &dirs,
        spec: &spec,
        claude_tiles: &has,
    };
    let Plan::Replace(_, _, steps) = plan(Harness::Claude, &inputs) else {
        panic!("a stale server is replaced");
    };
    apply(&steps, &|p, a| fake.run(p, a)).unwrap();
    assert_eq!(
        *fake.ran.borrow(),
        [
            "claude mcp remove tiles --scope user",
            "claude mcp add --scope user tiles -e TILES_DRAW=text -- /opt/tiles mcp",
        ]
    );
    assert_eq!(fake.has.borrow().as_deref(), Some(CLAUDE_ADDED));
    assert_eq!(
        plan(Harness::Claude, &inputs),
        Plan::Current(on_path("claude")),
        "a current server is left as it is"
    );
    let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
}

#[test]
fn a_failed_remove_adds_nothing_and_says_what_to_run() {
    let stale = CLAUDE_ADDED.replace("/opt/tiles", "/old/tiles");
    let fake = FakeClaude::new(&stale, true);
    let e = apply(&claude_replace(), &|p, a| fake.run(p, a)).unwrap_err();
    assert_eq!(
        e,
        "claude exited with exit status: 1; Claude Code keeps its old tiles server: run claude mcp remove tiles --scope user, then tiles setup"
    );
    assert_eq!(
        *fake.ran.borrow(),
        ["claude mcp remove tiles --scope user"],
        "no second server is added"
    );
    assert_eq!(fake.has.borrow().as_deref(), Some(stale.as_str()));
}

#[test]
fn a_harness_left_for_the_user_is_never_called_set_up() {
    let other = Plan::Other(
        PathBuf::from("/bin/claude"),
        "set up with another tiles, /old/tiles".into(),
    );
    let refused = Plan::Refused(
        PathBuf::from("/bin/opencode"),
        "holds settings of yours".into(),
    );
    for left in [other, refused] {
        let plans = vec![
            (Harness::Pi, Plan::Current(PathBuf::from("/bin/pi"))),
            (Harness::Claude, left),
        ];
        let (text, installed, ok) = drive(
            Args::default(),
            &report(plans, "terminal: Warp", &Copies::default(), &[]),
            None,
        );
        assert!(installed.is_empty() && ok);
        assert!(
            text.contains("Nothing for Tiles to change; see the lines above.\n"),
            "{text}"
        );
        assert!(!text.contains("already has it"), "{text}");
        assert!(!text.contains("[y/N]"), "{text}");
    }
}

#[test]
fn claude_codes_own_config_is_read_for_its_tiles_server_alone() {
    let configured = |text: &str| Registered::configured(text);
    let server = r#"{"oauthAccount":{"x":1},"mcpServers":{"other":{"command":"x","env":{"KEY":"secret"}},"tiles":{"type":"stdio","command":"/old/tiles","args":["mcp"],"env":{"TILES_DRAW":"text"}}}}"#;
    assert_eq!(
        configured(server),
        Ok(Some(Registered {
            scope: Some("user".into()),
            command: Some("/old/tiles".into()),
            args: vec!["mcp".into()],
            env: vec!["TILES_DRAW=text".into()],
        }))
    );
    assert_eq!(configured(r#"{"numStartups":3}"#), Ok(None));
    assert_eq!(configured(r#"{"mcpServers":{"other":{}}}"#), Ok(None));
    assert_eq!(
        configured(r#"{"mcpServers":{"tiles":{"type":"http","url":"https://x"}}}"#),
        Ok(Some(Registered {
            scope: Some("user".into()),
            ..Registered::default()
        }))
    );
    for odd in [
        "",
        "not json",
        "[]",
        r#"{"mcpServers":3}"#,
        r#"{"mcpServers":{"tiles":"x"}}"#,
        r#"{"mcpServers":{"tiles":{"command":3}}}"#,
        r#"{"mcpServers":{"tiles":{"command":"/t","args":"mcp"}}}"#,
        r#"{"mcpServers":{"tiles":{"command":"/t","env":{"TILES_DRAW":1}}}}"#,
    ] {
        assert_eq!(configured(odd), Err(()), "{odd}");
    }
}

#[test]
fn a_config_of_the_users_is_refused_never_planned() {
    let dirs = dirs("refused");
    let mine = dirs.config_home.join("opencode/config.json");
    crate::addons::write_file(&mine, br#"{"theme":"mine"}"#).unwrap();
    let inputs = Inputs {
        which: &|name| Some(on_path(name)),
        dirs: &dirs,
        spec: &spec,
        claude_tiles: &|_| None,
    };
    let Plan::Refused(_, why) = plan(Harness::OpenCode, &inputs) else {
        panic!("planned over the user's config.json");
    };
    assert!(why.contains("holds settings of yours"), "{why}");
    let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
}

/// A store that records each account read, and holds `url` and maybe a
/// key under `api-key@acme.omniapp.co`.
struct Store {
    url: Option<&'static str>,
    key: bool,
    read: RefCell<Vec<String>>,
}

impl SecretStore for Store {
    fn get(&self, account: &str) -> Result<Option<String>, ConfigError> {
        self.read.borrow_mut().push(account.to_string());
        Ok(match account {
            "url" => self.url.map(str::to_string),
            "api-key@acme.omniapp.co" if self.key => Some("omni_planted_key_7Yq2".into()),
            _ => None,
        })
    }

    fn has(&self, account: &str) -> Result<bool, ConfigError> {
        Ok(account == "api-key@acme.omniapp.co" && self.key)
    }
}

fn store(url: Option<&'static str>, key: bool) -> Store {
    Store {
        url,
        key,
        read: RefCell::new(Vec::new()),
    }
}

#[test]
fn omni_is_found_from_its_url_and_the_key_is_never_read() {
    let none = |_: &str| None;
    let absent = |_: &str| false;
    let keychain = store(Some("https://acme.omniapp.co"), true);
    assert_eq!(
        omni(&none, &absent, &keychain),
        (
            Found {
                line: "omni: set up for acme.omniapp.co, key in the Keychain".into(),
                next: Vec::new(),
            },
            true
        )
    );
    assert_eq!(*keychain.read.borrow(), ["url"], "only the URL is read");
    let no_key = store(Some("https://acme.omniapp.co/api"), false);
    assert_eq!(
        omni(&none, &absent, &no_key),
        (
            Found {
                line: "omni: the Keychain names acme.omniapp.co but holds no API key for it".into(),
                next: vec!["tiles omni set-key https://acme.omniapp.co".into()],
            },
            false
        )
    );
    assert_eq!(
        omni(&none, &absent, &store(None, false)).0.line,
        "omni: no Omni URL in the Keychain or TILES_OMNI_URL"
    );

    let url = |k: &str| (k == "TILES_OMNI_URL").then(|| "https://acme.omniapp.co".to_string());
    let key_set = |k: &str| k == "TILES_OMNI_API_KEY";
    let untouched = store(None, false);
    assert_eq!(
        omni(&url, &key_set, &untouched),
        (
            Found {
                line: "omni: set up for acme.omniapp.co, in the environment".into(),
                next: Vec::new(),
            },
            true
        )
    );
    assert!(untouched.read.borrow().is_empty());
    let (found, set_up) = omni(&url, &absent, &untouched);
    assert!(!set_up);
    assert_eq!(
        found.line,
        "omni: TILES_OMNI_URL names acme.omniapp.co but TILES_OMNI_API_KEY is not set"
    );
    assert!(
        found.next[0].ends_with("tiles omni set-key https://acme.omniapp.co"),
        "{found:?}"
    );
    assert_eq!(
        omni(&none, &key_set, &untouched).0.line,
        "omni: TILES_OMNI_API_KEY is set without TILES_OMNI_URL"
    );
}

fn plans() -> Vec<(Harness, Plan)> {
    vec![
        (Harness::Pi, Plan::Current(PathBuf::from("/bin/pi"))),
        (Harness::OpenCode, Plan::Missing),
        (
            Harness::Claude,
            Plan::Install(
                PathBuf::from("/bin/claude"),
                vec![Step::Run("claude".into(), vec!["mcp".into(), "add".into()])],
            ),
        ),
        (
            Harness::Codex,
            Plan::Install(
                PathBuf::from("/bin/codex"),
                vec![Step::Write(PathBuf::from("/c/SKILL.md"), b"x".to_vec())],
            ),
        ),
    ]
}

fn data() -> Vec<Found> {
    vec![Found {
        line: "warehouse: not set up; found a gcloud login at /h/adc.json".into(),
        next: vec!["tiles warehouse setup --billing-project <id> --project <id>".into()],
    }]
}

/// Runs the report with `args`, answering `reply`, and returns what it
/// printed, the harnesses it installed and whether it succeeded.
fn drive(args: Args, report: &Report, reply: Option<&str>) -> (String, Vec<Harness>, bool) {
    let asked = RefCell::new(false);
    let mut answer = || {
        *asked.borrow_mut() = true;
        reply.map(str::to_string)
    };
    let installed = RefCell::new(Vec::new());
    let apply = |h: Harness, _: &[Step]| {
        installed.borrow_mut().push(h);
        if h == Harness::Codex {
            Err("codex exited with exit status: 1".to_string())
        } else {
            Ok(())
        }
    };
    let mut out = Vec::new();
    let ok = run(args, report, false, &mut out, &mut answer, &apply).unwrap();
    let text = String::from_utf8(out).unwrap();
    if args.yes || args.dry_run {
        assert!(!*asked.borrow(), "asked despite {args:?}");
    }
    (text, installed.into_inner(), ok)
}

#[test]
fn it_lists_everything_asks_once_and_no_answer_or_no_changes_nothing() {
    let report = report(
        plans(),
        "terminal: Warp, nothing to change",
        &Copies::default(),
        &data(),
    );
    for reply in [None, Some("\n"), Some("n\n"), Some("no\n"), Some("yep\n")] {
        let (text, installed, ok) = drive(Args::default(), &report, reply);
        assert!(installed.is_empty(), "{reply:?} installed {installed:?}");
        assert!(ok);
        assert!(text.contains("Nothing changed."), "{text}");
        assert!(
            text.contains("Set up Tiles for Claude Code and Codex? [y/N] "),
            "{text}"
        );
    }
    let (text, installed, ok) = drive(Args::default(), &report, None);
    assert_eq!(
        text,
        "Tiles found:
  terminal: Warp, nothing to change
  pi: /bin/pi, already set up
  opencode: not on PATH
  claude: /bin/claude, not set up
  codex: /bin/codex, not set up
  warehouse: not set up; found a gcloud login at /h/adc.json
Tiles will change:
  claude: run claude mcp add
  codex: write /c/SKILL.md
Set up Tiles for Claude Code and Codex? [y/N]\u{20}
Nothing changed. Run tiles setup again to set it up, or tiles install <harness> for one harness.
To finish the data sources, run:
  tiles warehouse setup --billing-project <id> --project <id>
Then run tiles setup again, so each harness gets the data tools and asks before each Tiles tool call.
"
    );
    assert!(installed.is_empty() && ok);
}

#[test]
fn yes_installs_each_harness_and_reports_a_failure_without_stopping() {
    let report = report(plans(), "terminal: Warp", &Copies::default(), &data());
    for (args, reply) in [
        (Args::default(), Some("y\n")),
        (Args::default(), Some(" YES \n")),
        (
            Args {
                yes: true,
                dry_run: false,
            },
            None,
        ),
    ] {
        let (text, installed, ok) = drive(args, &report, reply);
        assert_eq!(installed, [Harness::Claude, Harness::Codex]);
        assert!(!ok, "codex failed");
        assert!(
            text.contains("installed Tiles for claude; tiles run claude adds the inline card"),
            "{text}"
        );
        assert!(
            text.contains("tiles: codex not set up: codex exited with exit status: 1"),
            "{text}"
        );
    }
}

#[test]
fn dry_run_prints_the_plan_and_changes_nothing() {
    let report = report(plans(), "terminal: Warp", &Copies::default(), &data());
    let (text, installed, ok) = drive(
        Args {
            yes: true,
            dry_run: true,
        },
        &report,
        Some("y\n"),
    );
    assert!(installed.is_empty() && ok);
    assert!(text.contains("  claude: run claude mcp add\n"), "{text}");
    assert!(text.contains("Dry run: nothing changed.\n"), "{text}");
    assert!(!text.contains("[y/N]"), "{text}");
}

#[test]
fn a_second_run_and_a_machine_with_no_harness_ask_nothing() {
    let current = vec![
        (Harness::Pi, Plan::Current(PathBuf::from("/bin/pi"))),
        (Harness::OpenCode, Plan::Missing),
        (Harness::Claude, Plan::Missing),
        (Harness::Codex, Plan::Missing),
    ];
    let (text, installed, ok) = drive(
        Args::default(),
        &report(current, "terminal: Warp", &Copies::default(), &[]),
        None,
    );
    assert!(installed.is_empty() && ok);
    assert!(
        text.contains("already has it; nothing to change."),
        "{text}"
    );
    assert!(
        !text.contains("[y/N]") && !text.contains("data sources"),
        "{text}"
    );
    let none = Harness::ALL.map(|h| (h, Plan::Missing)).to_vec();
    let (text, _, _) = drive(
        Args::default(),
        &report(none, "terminal: Warp", &Copies::default(), &[]),
        None,
    );
    assert!(
        text.contains("No harness Tiles supports is on PATH"),
        "{text}"
    );
}

#[test]
fn found_lines_are_one_plain_line_each() {
    let hostile = Found {
        line: "warehouse: \u{1b}]52;c;aGk=\u{7}found\nmore".into(),
        next: vec!["tiles \u{202e}x".into()],
    };
    let r = report(
        vec![(
            Harness::OpenCode,
            Plan::Refused(PathBuf::from("/bin/\u{1b}[2Jopencode"), "no\nway".into()),
        )],
        "terminal: Warp",
        &Copies {
            this: PathBuf::from("/usr/local/bin/tiles"),
            current: "0.1.9".into(),
            others: vec![Other {
                path: PathBuf::from("/h/\u{7}bin/tiles"),
                version: Some("0.1.2".into()),
                against: Some(std::cmp::Ordering::Less),
                place: Place::Ahead,
                yours: false,
                aside: Some(PathBuf::from("/h/\u{7}bin/tiles-0.1.2-old")),
            }],
        },
        &[hostile],
    );
    for line in r.found.iter().chain(&r.next) {
        assert!(!line.chars().any(tiles_intent::forbidden), "{line:?}");
        assert!(!line.contains('\n'), "{line:?}");
    }
}

#[test]
fn on_the_text_route_setup_says_cards_will_not_draw_inline_and_how_they_open() {
    let env = |vars: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    };
    let apple = env(&[("TERM_PROGRAM", "Apple_Terminal")]);
    let mut shown = report(
        plans(),
        "terminal: apple-terminal, nothing to change",
        &Copies::default(),
        &[],
    );
    add_cards(&mut shown, &apple);
    let (text, _, _) = drive(Args::default(), &shown, None);
    let here = if cfg!(target_os = "macos") {
        "tiles run shows each session's cards in one window beside this terminal (TILES_WINDOW=off keeps it closed)"
    } else {
        "each reply gives the card's PNG path"
    };
    assert!(
        text.starts_with(&format!(
            "Tiles found:
  terminal: apple-terminal, nothing to change
  pictures: none in this terminal, so cards will not draw inline here; Warp, Ghostty, kitty, iTerm2 and WezTerm draw them inline. Here {here}.
  pi: /bin/pi, already set up
"
        )),
        "{text}"
    );
    // A terminal that draws, and one setup cannot name, get no such line.
    for vars in [&[("TERM_PROGRAM", "WarpTerminal")][..], &[]] {
        let mut shown = report(plans(), "terminal: x", &Copies::default(), &[]);
        add_cards(&mut shown, &env(vars));
        assert_eq!(shown.found[1], "pi: /bin/pi, already set up", "{vars:?}");
    }
    // Forced to the text route, any terminal gets it.
    let mut shown = report(plans(), "terminal: x", &Copies::default(), &[]);
    add_cards(&mut shown, &env(&[("TILES_ROUTE", "text")]));
    assert!(shown.found[1].starts_with("pictures: none in this terminal"));
}

/// A machine with `tiles` `version` in `local`, ahead of the running one,
/// 0.1.9, in `usr` on `PATH`, and the copies setup finds there.
fn shadowed(name: &str, version: &str) -> (PathBuf, PathBuf, Copies) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let root = std::env::temp_dir().join(format!("tiles-setup-copy-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (bin, version) in [("local", version), ("usr", "0.1.9")] {
        std::fs::create_dir_all(root.join(bin)).unwrap();
        let p = root.join(bin).join("tiles");
        std::fs::write(
            &p,
            format!("#!/bin/sh\necho 'tiles {version} (intent schema 1)'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (old, this) = (root.join("local/tiles"), root.join("usr/tiles"));
    let path = std::env::join_paths([root.join("local"), root.join("usr")]).unwrap();
    let uid = std::fs::metadata(&this).unwrap().uid();
    let copies = crate::copies::find(&path, &this, "0.1.9", uid, &|p| {
        crate::opencode::ask_with(p, &["--version"], std::time::Duration::from_secs(30))
    });
    (root, old, copies)
}

/// Runs setup over `copies` with every harness already set up, `tty`
/// saying whether a person can answer, replying `reply`; returns what it
/// printed, whether it asked, and whether it succeeded.
fn drive_copies(
    args: Args,
    copies: &Copies,
    tty: bool,
    reply: Option<&str>,
) -> (String, bool, bool) {
    drive_plan(
        Plan::Current(PathBuf::from("/bin/pi")),
        args,
        copies,
        tty,
        reply,
    )
}

/// [`drive_copies`] with Pi's plan `pi`.
fn drive_plan(
    pi: Plan,
    args: Args,
    copies: &Copies,
    tty: bool,
    reply: Option<&str>,
) -> (String, bool, bool) {
    let r = report(vec![(Harness::Pi, pi)], "terminal: Warp", copies, &[]);
    let asked = RefCell::new(false);
    let mut answer = || {
        *asked.borrow_mut() = true;
        reply.map(str::to_string)
    };
    let mut out = Vec::new();
    let ok = run(args, &r, tty, &mut out, &mut answer, &|_, _| Ok(())).unwrap();
    (String::from_utf8(out).unwrap(), asked.into_inner(), ok)
}

#[test]
fn an_older_tiles_first_on_path_is_moved_aside_only_after_a_yes() {
    let (root, old, copies) = shadowed("yes", "0.1.2");
    let this = root.join("usr/tiles");
    let aside = root.join("local/tiles-0.1.2-old");
    let mv = format!("mv -n {} {}", old.display(), aside.display());
    for reply in [None, Some("\n"), Some("n\n")] {
        let (text, asked, ok) = drive_copies(Args::default(), &copies, true, reply);
        assert!(asked && ok);
        assert!(old.exists() && !aside.exists(), "{reply:?} moved it");
        assert_eq!(
            text,
            format!(
                "Tiles found:
  terminal: Warp
  tiles: {}, version 0.1.2, comes first on PATH, so a terminal runs it, not this one ({}, version 0.1.9)
  pi: /bin/pi, already set up
Tiles will change:
  tiles: move {} aside, to tiles-0.1.2-old beside it
Every harness on PATH that Tiles can set up already has it.
Move {} aside, to {}? [y/N] {}Left {} where it is; to move it aside, run: {mv}
",
                old.display(),
                this.display(),
                old.display(),
                old.display(),
                aside.display(),
                if reply.is_none() { "\n" } else { "" },
                old.display(),
            )
        );
    }
    let (text, _, ok) = drive_copies(Args::default(), &copies, true, Some("Y\n"));
    assert!(ok);
    assert!(!old.exists() && aside.exists());
    assert!(
        text.ends_with(&format!(
            "Moved {} to {}. To run {} as tiles, open a new terminal tab, or run hash -r (rehash in zsh).\n",
            old.display(),
            aside.display(),
            this.display()
        )),
        "{text}"
    );
    // Moved already: the name it wanted is taken, so the move fails, says
    // so, and replaces nothing.
    std::fs::write(&old, "new").unwrap();
    let (text, _, ok) = drive_copies(Args::default(), &copies, true, Some("y\n"));
    assert!(!ok, "{text}");
    assert!(
        text.contains(&format!(
            "tiles: could not move {}: {} exists now; run: {mv}",
            old.display(),
            aside.display()
        )),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&old).unwrap(), "new");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_a_terminal_or_with_yes_or_a_dry_run_setup_prints_the_move_and_asks_nothing() {
    let (root, old, copies) = shadowed("no-tty", "0.1.2");
    let mv = format!(
        "mv -n {} {}",
        old.display(),
        root.join("local/tiles-0.1.2-old").display()
    );
    let printed = format!(
        "To move {} aside, run: {mv}, then open a new terminal tab, or run hash -r (rehash in zsh).\n",
        old.display()
    );
    for (args, tty) in [
        (Args::default(), false),
        (
            Args {
                yes: true,
                dry_run: false,
            },
            true,
        ),
        (
            Args {
                yes: false,
                dry_run: true,
            },
            true,
        ),
    ] {
        let (text, asked, ok) = drive_copies(args, &copies, tty, Some("y\n"));
        assert!(!asked && ok, "{args:?}");
        assert!(
            text.ends_with(&format!(
                "Every harness on PATH that Tiles can set up already has it; nothing to change.\n{printed}"
            )),
            "{args:?}: {text}"
        );
        assert!(!text.contains("Tiles will change:"), "{args:?}: {text}");
        assert!(old.exists());
    }
    // With an install to make, --yes lists only the install, and a dry run
    // says it changed nothing before the command.
    let install = || Plan::Install(PathBuf::from("/bin/pi"), Vec::new());
    for (yes, dry_run) in [(true, false), (false, true)] {
        let (text, _, _) = drive_plan(install(), Args { yes, dry_run }, &copies, true, None);
        assert!(!text.contains("tiles: move "), "{text}");
        assert!(text.ends_with(&printed), "{text}");
        assert_eq!(
            text.contains("Dry run: nothing changed.\n"),
            dry_run,
            "{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn with_a_harness_left_the_move_is_the_one_change_setup_names() {
    let (root, old, copies) = shadowed("left", "0.1.2");
    let refused = Plan::Refused(PathBuf::from("/bin/pi"), "holds settings of yours".into());
    let (text, asked, ok) = drive_plan(refused, Args::default(), &copies, true, Some("n\n"));
    assert!(asked && ok);
    assert!(
        text.contains(&format!(
            "Tiles will change:
  tiles: move {} aside, to tiles-0.1.2-old beside it
Tiles changes no harness; see the lines above.
Move {} aside, to ",
            old.display(),
            old.display()
        )),
        "{text}"
    );
    assert!(!text.contains("Nothing for Tiles to change"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn declining_the_install_says_no_harness_changed_and_still_asks_the_move() {
    let (root, old, copies) = shadowed("decline", "0.1.2");
    let aside = root.join("local/tiles-0.1.2-old");
    let r = report(
        vec![(
            Harness::Pi,
            Plan::Install(PathBuf::from("/bin/pi"), Vec::new()),
        )],
        "terminal: Warp",
        &copies,
        &[],
    );
    for (move_reply, moved) in [("n\n", false), ("y\n", true)] {
        let mut replies = vec!["n\n", move_reply].into_iter();
        let mut answer = || replies.next().map(str::to_string);
        let applied = RefCell::new(false);
        let apply = |_: Harness, _: &[Step]| {
            *applied.borrow_mut() = true;
            Ok(())
        };
        let mut out = Vec::new();
        let ok = run(Args::default(), &r, true, &mut out, &mut answer, &apply).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(ok && !*applied.borrow(), "{text}");
        assert_eq!(replies.next(), None, "{text}");
        assert!(
            text.contains(&format!(
                "Tiles will change:
  tiles: move {} aside, to tiles-0.1.2-old beside it
Set up Tiles for Pi? [y/N] No harness changed. Run tiles setup again to set it up, or tiles install <harness> for one harness.
Move {} aside, to {}? [y/N] ",
                old.display(),
                old.display(),
                aside.display()
            )),
            "{text}"
        );
        assert!(!text.contains("Nothing changed"), "{text}");
        assert_eq!((old.exists(), aside.exists()), (!moved, moved), "{text}");
        assert_eq!(text.contains("? [y/N] Moved "), moved, "{text}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_newer_tiles_first_on_path_is_named_newer_and_never_offered() {
    let (root, newer, copies) = shadowed("newer", "0.2.0");
    for (args, tty) in [(Args::default(), true), (Args::default(), false)] {
        let (text, asked, ok) = drive_copies(args, &copies, tty, Some("y\n"));
        assert!(!asked && ok);
        assert!(newer.exists());
        assert_eq!(
            text,
            format!(
                "Tiles found:
  terminal: Warp
  tiles: {}, version 0.2.0, is newer than this one ({}, version 0.1.9) and comes first on PATH, so a terminal runs it
  pi: /bin/pi, already set up
Every harness on PATH that Tiles can set up already has it; nothing to change.
",
                newer.display(),
                root.join("usr/tiles").display()
            )
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
