//! `tiles run --dry-run claude` with a `--settings` of the user's own
//! (ADR-028): Claude Code takes one `--settings`, so the launch names only
//! the file Tiles writes, with the user's settings and Tiles' deny rules
//! merged, and the dry run prints its contents. A value that cannot be read
//! or parsed refuses the launch with one line naming it.

use std::path::Path;
use std::process::{Command, Output};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

fn dry_run(dir: &Path, args: &[&str]) -> Output {
    Command::new(TILES)
        .args(["run", "--dry-run", "--terminal", "warp", "claude"])
        .args(args)
        .env("TILES_CACHE_DIR", dir.join("cache"))
        .env("TILES_CONFIG_DIR", dir.join("config"))
        .env_remove("TILES_OMNI_URL")
        .env_remove("TILES_OMNI_API_KEY")
        .env_remove("TILES_DRAW")
        .output()
        .unwrap()
}

fn text_of(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The launch's arguments and the settings file's contents, from the dry
/// run's `arg:` and `file:` lines.
fn planned(out: &Output) -> (Vec<String>, serde_json::Value) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let args = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("  arg: "))
        .map(str::to_string)
        .collect();
    let files: Vec<&str> = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("  file: "))
        .collect();
    assert_eq!(files.len(), 1, "{stdout}");
    let (path, json) = files[0].split_once(" = ").unwrap();
    assert!(path.ends_with("/claude-settings.json"), "{path}");
    (args, serde_json::from_str(json).unwrap())
}

#[test]
fn a_users_settings_file_or_json_is_merged_and_passed_once() {
    let dir = std::env::temp_dir().join(format!("tiles-run-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cache = dir.join("cache").display().to_string();
    let read_rule = format!("Read(/{cache}/**)");
    let bash_rule = format!("Bash(*{cache}*)");

    let mine = dir.join("mine.json");
    std::fs::write(
        &mine,
        r#"{"permissions":{"deny":["Read(//secret/**)"],"allow":["Bash(ls *)"]},"model":"opus"}"#,
    )
    .unwrap();
    let out = dry_run(&dir, &["-p", "hi", "--settings", mine.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text_of(&out));
    let (args, settings) = planned(&out);
    assert_eq!(args[..2], ["-p", "hi"]);
    assert_eq!(
        args.iter().filter(|a| *a == "--settings").count(),
        1,
        "{args:?}"
    );
    assert!(
        !args.contains(&mine.to_str().unwrap().to_string()),
        "{args:?}"
    );
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert_eq!(deny[0], "Read(//secret/**)", "the user's rules come first");
    assert!(deny.contains(&serde_json::json!(read_rule)), "{deny:?}");
    assert!(deny.contains(&serde_json::json!(bash_rule)), "{deny:?}");
    assert_eq!(
        settings["permissions"]["allow"],
        serde_json::json!(["Bash(ls *)"])
    );
    assert_eq!(settings["model"], "opus");
    assert!(settings.get("sandbox").is_none(), "{settings}");

    let inline = format!(r#"{{"permissions":{{"deny":["{read_rule}"]}},"theme":"light"}}"#);
    let out = dry_run(&dir, &["--settings", &inline]);
    assert!(out.status.success(), "{}", text_of(&out));
    let (args, settings) = planned(&out);
    assert_eq!(args.iter().filter(|a| *a == "--settings").count(), 1);
    assert!(!args.contains(&inline), "{args:?}");
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert_eq!(
        deny.iter()
            .filter(|r| **r == serde_json::json!(read_rule))
            .count(),
        1,
        "a rule the user already has is not added again: {deny:?}"
    );
    assert_eq!(settings["theme"], "light");

    let plain = dry_run(&dir, &[]);
    assert!(plain.status.success(), "{}", text_of(&plain));
    let (_, settings) = planned(&plain);
    assert_eq!(
        settings["permissions"]["deny"].as_array().unwrap()[..2],
        [serde_json::json!(read_rule), serde_json::json!(bash_rule)]
    );

    let missing = dir.join("none.json");
    let out = dry_run(&dir, &["--settings", missing.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(
        err.starts_with(&format!(
            "tiles: cannot read --settings {}: ",
            missing.display()
        )),
        "{err}"
    );
    let out = dry_run(&dir, &["--settings", "{oops"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.starts_with("tiles: cannot parse --settings {oops: "),
        "{err}"
    );
    assert_eq!(err.lines().count(), 1, "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_long_settings_file_is_shown_whole() {
    let dir = std::env::temp_dir().join(format!("tiles-run-long-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let allow: Vec<String> = (0..300).map(|i| format!("Bash(tool-{i:03} *)")).collect();
    let mine = dir.join("mine.json");
    let settings = serde_json::json!({ "permissions": { "allow": allow } });
    let text = settings.to_string();
    assert!(text.len() > 4000, "{} characters", text.len());
    std::fs::write(&mine, &text).unwrap();
    let out = dry_run(&dir, &["--settings", mine.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text_of(&out));
    let (_, shown) = planned(&out);
    assert_eq!(
        shown["permissions"]["allow"],
        settings["permissions"]["allow"]
    );
    let cache = dir.join("cache").display().to_string();
    assert_eq!(
        shown["permissions"]["deny"].as_array().unwrap()[..2],
        [
            serde_json::json!(format!("Read(/{cache}/**)")),
            serde_json::json!(format!("Bash(*{cache}*)")),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ADR-032: a dry run prints the plan it always did on stdout; a Claude
/// Code of another minor version than Tiles was proved on adds one line on
/// stderr, and one whose version cannot be read adds nothing.
#[test]
fn a_dry_run_names_an_unproved_harness_version_and_nothing_else() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = std::env::temp_dir().join(format!("tiles-run-version-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let claude = dir.join("bin/claude");
    let run = |answer: &str| {
        std::fs::write(&claude, format!("#!/bin/sh\n{answer}\n")).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = Command::new(TILES)
            .args(["run", "--dry-run", "--terminal", "warp", "claude"])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", dir.join("bin").display()),
            )
            .env("TILES_CACHE_DIR", dir.join("cache"))
            .env("TILES_CONFIG_DIR", dir.join("config"))
            .env_remove("TILES_OMNI_URL")
            .env_remove("TILES_OMNI_API_KEY")
            .env_remove("TILES_DRAW")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text_of(&out));
        // The add-ons directory and the session token change per run.
        let blank = |text: &str, after: &str, keep: fn(char) -> bool| {
            text.split(after)
                .enumerate()
                .map(|(i, part)| {
                    if i == 0 {
                        part.to_string()
                    } else {
                        part.trim_start_matches(keep).to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(&format!("{after}N"))
        };
        let plan: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| blank(l, "/run/", |c| c.is_ascii_digit()))
            .map(|l| blank(&l, "TILES_SESSION\":\"", |c| c.is_ascii_hexdigit()))
            .map(|l| blank(&l, "TILES_SESSION=", |c| c.is_ascii_hexdigit()))
            .collect();
        (plan, String::from_utf8_lossy(&out.stderr).into_owned())
    };
    let (proved, said) = run("echo '2.1.308 (Claude Code)'");
    assert_eq!(said, "");
    assert!(proved.iter().any(|l| l == "program: claude"), "{proved:?}");
    let (newer, said) = run("echo '2.2.0 (Claude Code)'");
    assert_eq!(newer, proved);
    assert_eq!(
        said,
        "Tiles was proved on Claude Code 2.1.308, and this is 2.2.0; if a card misdraws, run tiles check\n"
    );
    let (unread, said) = run("exit 1");
    assert_eq!(unread, proved);
    assert_eq!(said, "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ADR-032: a harness that clears the screen on start wipes the line
/// printed before it, so `tiles run` prints the same line again once the
/// harness exits.
#[test]
fn the_version_line_shows_again_after_the_harness_exits() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = std::env::temp_dir().join(format!("tiles-run-after-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let claude = dir.join("bin/claude");
    std::fs::write(
        &claude,
        "#!/bin/sh\ncase \"$1\" in --version) echo '2.2.0 (Claude Code)' ;; *) echo harness ran >&2 ;; esac\n",
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(TILES)
        .args(["run", "--terminal", "warp", "claude"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", dir.join("bin").display()),
        )
        .env("TILES_CACHE_DIR", dir.join("cache"))
        .env("TILES_CONFIG_DIR", dir.join("config"))
        .env_remove("TILES_OMNI_URL")
        .env_remove("TILES_OMNI_API_KEY")
        .env_remove("TILES_DRAW")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text_of(&out));
    let line = "Tiles was proved on Claude Code 2.1.308, and this is 2.2.0; if a card misdraws, run tiles check";
    let said: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(said, [line, "harness ran", line], "{said:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
