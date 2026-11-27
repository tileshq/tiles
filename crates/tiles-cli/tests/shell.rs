//! The shell wiring end to end (ADR-069): `tiles setup` wires zsh after its
//! one question and a second run changes nothing, `tiles shell` and `tiles
//! doctor` say so, and `tiles shell off` gives the `.zshrc` back byte for
//! byte. `tiles run --from-shell` passes a one-shot use to the harness
//! untouched, starts an interactive one with Tiles loaded, starts the harness
//! plain when Tiles fails before it, and keeps the harness's exit status.
//! The script setup writes runs the harness plain when its input is not a
//! terminal, in each of zsh and bash found on this machine.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

/// A machine with a stub `pi` on `PATH` that prints its arguments and
/// `TILES_RUN`, and exits with `PI_EXIT`.
fn machine(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tiles-shell-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["home", "path", "config", "cache"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let pi = root.join("path/pi");
    std::fs::write(
        &pi,
        "#!/bin/sh\necho \"pi [$*] TILES_RUN=[$TILES_RUN]\"\nexit \"${PI_EXIT:-0}\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
    root
}

/// The environment `tiles` runs in on the machine: zsh as the login shell.
fn command(root: &Path, program: &str) -> Command {
    let mut c = Command::new(program);
    c.env_clear()
        .env("HOME", root.join("home"))
        .env("SHELL", "/bin/zsh")
        .env(
            "PATH",
            format!(
                "{}:{}:/usr/bin:/bin",
                root.join("path").display(),
                Path::new(TILES).parent().unwrap().display()
            ),
        )
        .env("TILES_CONFIG_DIR", root.join("config"))
        .env("TILES_STATE_DIR", root.join("config"))
        .env("TILES_CACHE_DIR", root.join("cache"))
        .env("TILES_UPDATE_CHECK", "off")
        .env("TERM_PROGRAM", "WarpTerminal")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

fn tiles(root: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = command(root, TILES)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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

#[test]
fn setup_wires_zsh_once_and_shell_off_gives_the_zshrc_back() {
    let root = machine("setup");
    let zshrc = root.join("home/.zshrc");
    let script = root.join("config/shell.sh");
    let before = b"export EDITOR=vim\nalias ll='ls -l'";
    std::fs::write(&zshrc, before).unwrap();

    let out = tiles(&root, &["setup"], Some("n\n"));
    let said = text(&out);
    assert!(
        said.contains("Set up Tiles for Pi and zsh? [y/N] "),
        "{said}"
    );
    assert!(
        said.contains("  Shell          (skipped: not confirmed)"),
        "{said}"
    );
    assert_eq!(std::fs::read(&zshrc).unwrap(), before);
    assert!(!script.exists());

    let out = tiles(&root, &["setup"], Some("y\n"));
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("  Shell          (wired zsh; open a new terminal tab to use it)"),
        "{said}"
    );
    assert!(
        said.ends_with("Next: run tiles check, then open a new terminal tab and type pi.\n"),
        "{said}"
    );
    let wired = std::fs::read_to_string(&zshrc).unwrap();
    assert_eq!(
        wired,
        format!(
            "export EDITOR=vim\nalias ll='ls -l'\n# Added by tiles setup: plain claude, codex, opencode and pi draw cards; tiles shell off removes it\nif [ -r '{0}' ]; then . '{0}'; fi\n",
            script.display()
        )
    );
    assert!(
        std::fs::read_to_string(&script)
            .unwrap()
            .contains("function pi {")
    );

    let out = tiles(&root, &["setup"], None);
    let said = text(&out);
    assert!(
        said.contains("  Shell          (already wired for zsh)"),
        "{said}"
    );
    assert!(!said.contains("[y/N]"), "{said}");
    assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), wired);

    let status = text(&tiles(&root, &["shell"], None));
    assert!(
        status.starts_with(&format!(
            "shell: wired for zsh through {}: ",
            zshrc.display()
        )),
        "{status}"
    );
    let doctor = text(&tiles(&root, &["doctor"], None));
    assert!(
        doctor.contains("\n  shell: wired for zsh through "),
        "{doctor}"
    );

    let out = tiles(&root, &["shell", "off", "--dry-run"], None);
    assert!(
        text(&out).contains("would remove the Tiles lines from"),
        "{}",
        text(&out)
    );
    assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), wired);

    let out = tiles(&root, &["shell", "off"], None);
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(&format!("deleted {}", script.display())),
        "{said}"
    );
    assert_eq!(std::fs::read(&zshrc).unwrap(), before, "byte for byte");
    assert!(!script.exists());
    let said = text(&tiles(&root, &["shell", "off"], None));
    assert!(said.starts_with("Nothing to remove"), "{said}");
    let doctor = text(&tiles(&root, &["doctor"], None));
    assert!(
        doctor.contains("\n  shell: not wired for zsh: "),
        "{doctor}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_one_shot_use_from_the_shell_reaches_the_harness_untouched() {
    let root = machine("aside");
    let out = tiles(
        &root,
        &["run", "--from-shell", "pi", "-p", "a b", "--mode", "json"],
        None,
    );
    assert_eq!(text(&out), "pi [-p a b --mode json] TILES_RUN=[]\n");
    assert!(out.status.success());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_interactive_session_from_the_shell_starts_with_tiles_loaded() {
    let root = machine("loaded");
    let out = tiles(&root, &["run", "--from-shell", "pi", "--model", "x"], None);
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    let line = said.lines().find(|l| l.starts_with("pi [")).unwrap();
    assert!(line.starts_with("pi [-e "), "{said}");
    assert!(
        line.ends_with("/pi/index.ts --model x] TILES_RUN=[1]"),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_tiles_that_fails_before_the_harness_starts_it_plain() {
    let root = machine("broken");
    // A cache that is a file: tiles run cannot open it.
    let cache = root.join("cache-file");
    std::fs::write(&cache, "").unwrap();
    let out = command(&root, TILES)
        .args(["run", "--from-shell", "pi", "--model", "x"])
        .env("TILES_CACHE_DIR", &cache)
        .env("PI_EXIT", "3")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let said = text(&out);
    assert_eq!(out.status.code(), Some(3), "{said}");
    assert!(
        String::from_utf8_lossy(&out.stderr).ends_with("; starting pi without Tiles\n"),
        "{said}"
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "pi [--model x] TILES_RUN=[]\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_exit_status_and_a_missing_harness_read_as_the_shell_would() {
    let root = machine("status");
    let out = command(&root, TILES)
        .args(["run", "--from-shell", "pi", "-p", "x"])
        .env("PI_EXIT", "7")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    let out = command(&root, TILES)
        .args(["run", "--from-shell", "pi"])
        .env("PI_EXIT", "5")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5), "{}", text(&out));
    let out = tiles(&root, &["run", "--from-shell", "codex"], None);
    assert_eq!(out.status.code(), Some(127));
    assert_eq!(text(&out), "codex: command not found\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_script_runs_the_harness_plain_when_its_input_is_not_a_terminal() {
    let root = machine("script");
    assert!(tiles(&root, &["setup", "--yes"], None).status.success());
    let script = root.join("config/shell.sh");
    let mut ran = 0;
    for shell in ["/bin/zsh", "/bin/bash", "/usr/bin/zsh", "/usr/bin/bash"] {
        if !Path::new(shell).exists() {
            continue;
        }
        let out = command(&root, shell)
            .arg("-c")
            .arg(format!(". '{}' && type pi && pi 'a b' c", script.display()))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let said = text(&out);
        assert!(out.status.success(), "{shell}: {said}");
        assert!(said.contains("function"), "{shell}: {said}");
        assert!(
            said.ends_with("pi [a b c] TILES_RUN=[]\n"),
            "{shell}: {said}"
        );
        ran += 1;
    }
    assert!(ran > 0, "no zsh or bash here");
    let _ = std::fs::remove_dir_all(&root);
}
