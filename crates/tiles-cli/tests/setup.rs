//! `tiles setup` end to end (ADR-035): the binary under test in a temporary
//! `HOME`, with stub `pi`, `opencode`, `claude` and `codex` first on `PATH`
//! that log every call and write what the real `mcp add` commands write.
//! No answer, or no, changes nothing; yes sets up every harness through the
//! steps `tiles install` runs; a second run finds each one set up and
//! changes nothing; a user-scope Claude Code server for another `tiles` is
//! replaced after the same question (ADR-039), one in a project's config is
//! named and left, and a failed remove adds nothing; `tiles doctor` offers
//! `tiles setup` until it has run, reading Claude Code's registration from
//! `~/.claude.json` alone. An older `tiles` ahead of the one running on
//! `PATH` is named, with no terminal left where it is with the `mv` command
//! printed and no move listed, and named again by `tiles doctor`.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The stub `claude`'s `claude_has <command> <KEY=value>`, which records a
/// `tiles` server as `claude mcp get tiles` (Claude Code 2.1.304) prints it.
const CLAUDE_HAS: &str = "claude_has() {
  printf 'tiles:\\n  Scope: User config (available in all your projects)\\n  Type: stdio\\n  Command: %s\\n  Args: mcp\\n  Environment:\\n    %s\\n' \"$1\" \"$2\" > \"$(dirname \"$0\")/../claude-has-tiles\"
}";

/// A machine with the four harnesses on `PATH` and nothing set up.
fn machine(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tiles-setup-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["home", "path", "config"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let log = format!("echo \"$(basename \"$0\") $*\" >> '{}/log'", root.display());
    script(
        &root.join("path/claude"),
        &format!(
            "{log}
{CLAUDE_HAS}
case \"$1 $2\" in
  \"--version \") echo '2.1.304 (Claude Code)' ;;
  \"mcp get\") cat '{root}/claude-has-tiles' 2>/dev/null || {{ echo 'No MCP server named \"tiles\".'; exit 1; }} ;;
  \"mcp add\")
    # As Claude Code 2.1.304 reads it: `-e <env...>` takes every word up to
    # the next option, and each must be KEY=value.
    shift 2; name=; env=
    while [ \"$1\" != -- ]; do
      case \"$1\" in
        --scope) shift 2 ;;
        -e) shift
          while [ $# -gt 0 ] && [ \"${{1#-}}\" = \"$1\" ]; do
            case \"$1\" in *=*) env=\"$1\" ;; *) echo \"Invalid environment variable format: $1\" >&2; exit 1 ;; esac
            shift
          done ;;
        *) name=\"$1\"; shift ;;
      esac
    done
    [ \"$name\" = tiles ] || exit 1
    claude_has \"$2\" \"$env\" ;;
  \"mcp remove\") [ ! -e '{root}/claude-remove-fails' ] && rm '{root}/claude-has-tiles' ;;
esac",
            root = root.display()
        ),
    );
    script(
        &root.join("path/codex"),
        &format!(
            "{log}
case \"$1 $2\" in
  \"--version \") echo 'codex-cli 0.163.0' ;;
  \"mcp add\")
    while [ \"$1\" != -- ]; do shift; done
    mkdir -p \"$HOME/.codex\"
    printf '[mcp_servers.tiles]\\ncommand = \"%s\"\\nargs = [\"mcp\"]\\n\\n[mcp_servers.tiles.env]\\nTILES_DRAW = \"text\"\\n' \"$2\" > \"$HOME/.codex/config.toml\" ;;
esac"
        ),
    );
    script(&root.join("path/pi"), &format!("{log}\necho 0.70.0"));
    script(&root.join("path/opencode"), &format!("{log}\necho 1.19.3"));
    root
}

/// Runs `tiles <args>` on the machine with `stdin` as its input, `None`
/// for none at all.
fn tiles(root: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(TILES)
        .args(args)
        .env_clear()
        .env("HOME", root.join("home"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("path").display()),
        )
        .env("TILES_CONFIG_DIR", root.join("config"))
        .env("TILES_STATE_DIR", root.join("config"))
        .env("TILES_UPDATE_CHECK", "off")
        .env("TERM_PROGRAM", "WarpTerminal")
        // An Omni URL with no key: Omni is found but not set up, and the
        // Keychain is never asked.
        .env("TILES_OMNI_URL", "https://acme.omniapp.co")
        .current_dir(root)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        use std::io::Write as _;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn log(root: &Path) -> String {
    std::fs::read_to_string(root.join("log")).unwrap_or_default()
}

/// Every file an install writes under `HOME`.
fn installed(root: &Path) -> Vec<&'static str> {
    [
        ".pi/agent/extensions/tiles/index.ts",
        ".pi/agent/extensions/tiles/tool.json",
        ".config/opencode/config.json",
        ".config/opencode/tiles-instructions.md",
        ".codex/config.toml",
        ".codex/skills/tiles-theme/SKILL.md",
    ]
    .into_iter()
    .filter(|f| root.join("home").join(f).exists())
    .collect()
}

#[test]
fn no_answer_or_no_changes_nothing() {
    let root = machine("declined");
    for stdin in [None, Some(""), Some("n\n"), Some("N\n")] {
        let out = tiles(&root, &["setup"], stdin);
        let said = text(&out);
        assert!(out.status.success(), "{said}");
        assert!(
            said.contains("Set up Tiles for Pi, OpenCode, Claude Code and Codex? [y/N] "),
            "{said}"
        );
        assert!(said.contains("Nothing changed."), "{said}");
        assert_eq!(installed(&root), Vec::<&str>::new(), "{stdin:?}");
        assert!(!log(&root).contains("mcp add"), "{}", log(&root));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn yes_sets_up_every_harness_and_a_second_run_changes_nothing() {
    let root = machine("accepted");
    let doctor = text(&tiles(&root, &["doctor"], None));
    assert!(
        doctor.contains(
            "  setup: Tiles is not set up, or is out of date, for Pi, OpenCode and Codex; run tiles setup"
        ),
        "{doctor}"
    );

    let dry = tiles(&root, &["setup", "--dry-run"], None);
    assert!(
        text(&dry).contains("Dry run: nothing changed."),
        "{}",
        text(&dry)
    );
    assert_eq!(installed(&root), Vec::<&str>::new());

    let out = tiles(&root, &["setup"], Some("y\n"));
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    // A build without the warehouse source (ADR-059) says nothing about one.
    let warehouse = "  warehouse: no gcloud login, dbt project or dbt profiles.yml found";
    if text(&tiles(&root, &["--help"], None)).contains("tiles warehouse") {
        assert!(said.contains(warehouse), "{said}");
    } else {
        assert!(!said.contains("warehouse"), "{said}");
    }
    for line in [
        "  terminal: warp, nothing to change",
        "  pi: ",
        "  omni: TILES_OMNI_URL names acme.omniapp.co but TILES_OMNI_API_KEY is not set",
        &format!(
            "  claude: run claude mcp add --scope user tiles -e TILES_DRAW=text -- {TILES} mcp"
        ),
        "installed Tiles for pi; tiles run pi adds the inline card",
        "installed Tiles for opencode;",
        "installed Tiles for claude;",
        "installed Tiles for codex;",
        "To finish the data sources, run:",
    ] {
        assert!(said.contains(line), "{line}: {said}");
    }
    assert_eq!(installed(&root).len(), 6, "{:?}", installed(&root));
    assert!(log(&root).contains(&format!(
        "claude mcp add --scope user tiles -e TILES_DRAW=text -- {TILES} mcp"
    )));
    assert!(log(&root).contains(&format!(
        "codex mcp add tiles --env TILES_DRAW=text -- {TILES} mcp"
    )));

    let before = log(&root).matches("mcp add").count();
    let again = tiles(&root, &["setup"], None);
    let said = text(&again);
    assert!(again.status.success(), "{said}");
    for h in ["pi", "opencode", "claude", "codex"] {
        assert!(
            said.contains(&format!("  {h}: ")) && said.contains(", already set up"),
            "{said}"
        );
    }
    assert_eq!(said.matches(", already set up").count(), 4, "{said}");
    assert!(said.contains("nothing to change."), "{said}");
    assert!(!said.contains("[y/N]"), "{said}");
    assert_eq!(log(&root).matches("mcp add").count(), before);

    let doctor = text(&tiles(&root, &["doctor"], None));
    assert!(!doctor.contains("run tiles setup"), "{doctor}");
    let _ = std::fs::remove_dir_all(&root);
}

/// What `claude mcp get tiles` prints of a server for `/old/tiles` kept in
/// `scope`'s config.
fn old_server(scope: &str) -> String {
    format!(
        "tiles:\n  Scope: {scope}\n  Type: stdio\n  Command: /old/tiles\n  Args: mcp\n  Environment:\n    TILES_DRAW=text\n"
    )
}

#[test]
fn a_user_scope_claude_server_for_another_tiles_is_replaced_after_the_question() {
    let root = machine("claude-other");
    let has = root.join("claude-has-tiles");
    std::fs::write(
        &has,
        old_server("User config (available in all your projects)"),
    )
    .unwrap();
    let claude = root.join("path/claude");
    let found = format!(
        "  claude: {}, set up with another tiles, /old/tiles; Tiles will replace it",
        claude.display()
    );
    let out = tiles(&root, &["setup"], Some("n\n"));
    let said = text(&out);
    assert!(said.contains(&found), "{said}");
    assert!(
        said.contains("  claude: run claude mcp remove tiles --scope user\n"),
        "{said}"
    );
    assert!(said.contains("Nothing changed."), "{said}");
    assert!(!log(&root).contains("claude mcp remove"), "{}", log(&root));
    assert!(!log(&root).contains("claude mcp add"), "{}", log(&root));

    let out = tiles(&root, &["setup"], Some("y\n"));
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("installed Tiles for claude;"), "{said}");
    let log = log(&root);
    let ran: Vec<&str> = log
        .lines()
        .filter(|l| l.starts_with("claude mcp remove") || l.starts_with("claude mcp add"))
        .collect();
    assert_eq!(
        ran,
        [
            "claude mcp remove tiles --scope user",
            &format!("claude mcp add --scope user tiles -e TILES_DRAW=text -- {TILES} mcp"),
        ]
    );
    let again = text(&tiles(&root, &["setup"], None));
    assert!(
        again.contains(&format!("  claude: {}, already set up", claude.display())),
        "{again}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_project_scope_claude_server_is_named_and_left() {
    let root = machine("claude-project");
    let server = old_server("Project config (shared via .mcp.json)");
    std::fs::write(root.join("claude-has-tiles"), &server).unwrap();
    let out = tiles(&root, &["setup", "--yes"], None);
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(&format!(
            "  claude: {}, set up with another tiles, /old/tiles, in Claude Code's project config; Tiles leaves it: run claude mcp remove tiles --scope project, then tiles setup",
            root.join("path/claude").display()
        )),
        "{said}"
    );
    assert!(!said.contains("claude: run claude mcp"), "{said}");
    assert!(!log(&root).contains("claude mcp add"), "{}", log(&root));
    assert!(!log(&root).contains("claude mcp remove"), "{}", log(&root));
    assert_eq!(
        std::fs::read_to_string(root.join("claude-has-tiles")).unwrap(),
        server
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failed_remove_leaves_claude_code_as_it_was() {
    let root = machine("claude-remove-fails");
    let server = old_server("User config (available in all your projects)");
    std::fs::write(root.join("claude-has-tiles"), &server).unwrap();
    std::fs::write(root.join("claude-remove-fails"), "").unwrap();
    let out = tiles(&root, &["setup", "--yes"], None);
    let said = text(&out);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains(
            "tiles: claude not set up: claude exited with exit status: 1; Claude Code keeps its old tiles server: run claude mcp remove tiles --scope user, then tiles setup"
        ),
        "{said}"
    );
    assert!(log(&root).contains("claude mcp remove"), "{}", log(&root));
    assert!(!log(&root).contains("claude mcp add"), "{}", log(&root));
    assert_eq!(
        std::fs::read_to_string(root.join("claude-has-tiles")).unwrap(),
        server
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn yes_answers_for_a_script() {
    let root = machine("scripted");
    let out = tiles(&root, &["setup", "--yes"], None);
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(!said.contains("[y/N]"), "{said}");
    assert_eq!(installed(&root).len(), 6, "{said}");
    let bad = tiles(&root, &["setup", "--now"], None);
    assert_eq!(bad.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn doctor_reads_claude_codes_config_and_starts_no_server() {
    let root = machine("doctor-claude");
    let doctor = |config: Option<&str>| {
        let file = root.join("home/.claude.json");
        match config {
            Some(text) => std::fs::write(&file, text).unwrap(),
            None => {
                let _ = std::fs::remove_file(&file);
            }
        }
        let said = text(&tiles(&root, &["doctor"], None));
        said.lines()
            .filter(|l| l.starts_with("  setup: "))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let without_claude = [
        "  setup: Tiles is not set up, or is out of date, for Pi, OpenCode and Codex; run tiles setup",
    ];
    assert_eq!(
        doctor(None),
        without_claude,
        "no file: nothing said of Claude Code"
    );
    assert_eq!(
        doctor(Some("[]")),
        without_claude,
        "another shape: nothing said"
    );
    assert_eq!(
        doctor(Some(r#"{"mcpServers":{"other":{"command":"x"}}}"#)),
        [
            "  setup: Tiles is not set up, or is out of date, for Pi, OpenCode, Claude Code and Codex; run tiles setup"
        ]
    );
    assert_eq!(
        doctor(Some(
            r#"{"mcpServers":{"tiles":{"type":"stdio","command":"/old/tiles","args":["mcp"],"env":{"TILES_DRAW":"text"}}}}"#
        )),
        [
            without_claude[0],
            "  setup: Claude Code is set up with another tiles, /old/tiles; run tiles setup to replace it",
        ]
    );
    let this = format!(
        r#"{{"mcpServers":{{"tiles":{{"type":"stdio","command":"{TILES}","args":["mcp"],"env":{{"TILES_DRAW":"text"}}}}}}}}"#
    );
    assert_eq!(doctor(Some(&this)), without_claude, "this tiles is set up");
    assert!(!log(&root).contains("claude mcp"), "{}", log(&root));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_older_tiles_ahead_on_path_is_named_and_without_a_terminal_left_with_its_mv() {
    let root = machine("old-copy");
    for d in ["old", "new"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let old = root.join("old/tiles");
    script(&old, "echo 'tiles 0.1.2 (intent schema 1)'");
    std::os::unix::fs::symlink(TILES, root.join("new/tiles")).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(root.join("new/tiles"))
            .args(args)
            .env_clear()
            .env("HOME", root.join("home"))
            .env(
                "PATH",
                format!(
                    "{}:{}:{}:/usr/bin:/bin",
                    root.join("old").display(),
                    root.join("new").display(),
                    root.join("path").display()
                ),
            )
            .env("TILES_CONFIG_DIR", root.join("config"))
            .env("TILES_STATE_DIR", root.join("config"))
            .env("TILES_UPDATE_CHECK", "off")
            .env("TERM_PROGRAM", "WarpTerminal")
            .current_dir(&root)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        text(&out)
    };
    let aside = root.join("old/tiles-0.1.2-old");
    let mv = format!("mv -n {} {}", old.display(), aside.display());
    let setup = run(&["setup"]);
    assert!(
        setup.contains(&format!(
            "  tiles: {}, version 0.1.2, comes first on PATH, so a terminal runs it, not this one (",
            old.display()
        )),
        "{setup}"
    );
    assert!(!setup.contains("tiles: move "), "{setup}");
    assert!(
        setup.contains(&format!(
            "To move {} aside, run: {mv}, then ",
            old.display()
        )),
        "{setup}"
    );
    assert!(!setup.contains("? [y/N] y"), "{setup}");
    assert!(old.exists() && !aside.exists(), "moved without a terminal");
    let doctor = run(&["doctor"]);
    assert!(
        doctor.lines().nth(1).is_some_and(|l| l
            .starts_with(&format!("  tiles: {}, version 0.1.2", old.display()))
            && l.ends_with(&format!("; move it aside: {mv}"))),
        "{doctor}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
