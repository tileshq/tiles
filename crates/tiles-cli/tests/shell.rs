//! The shell wiring end to end (ADR-069): `tiles setup` wires zsh after its
//! one question and a second run changes nothing, `tiles doctor` says so,
//! and `tiles shell off` gives the `.zshrc` back byte for byte. `tiles run
//! --from-shell` passes a one-shot use to the harness untouched, starts an
//! interactive one with Tiles loaded, starts the harness plain when Tiles
//! fails before it, and keeps the harness's exit status. The script setup
//! writes, run by each of zsh, bash and fish found on this machine, starts a
//! harness typed in a terminal through the `tiles` that ran setup, with no
//! `tiles` on `PATH` or an older one ahead of it, and starts it plain when
//! its input is not a terminal, inside `tiles run`, or once that `tiles` is
//! gone. A harness not on `PATH` at setup gets no function, so the shell
//! runs it itself, until setup runs again, and a `pi` that does not answer
//! `--version` as Pi never gets one.

use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

/// A machine with a stub `pi` on `PATH` that prints its arguments and
/// `TILES_RUN`, and exits with `PI_EXIT`.
fn machine(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tiles-shell-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["home", "path", "config", "cache"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    stub_pi(&root);
    root
}

/// The stub `pi` on the machine's `PATH`: it answers `--version` as Pi
/// 1.4.0, and otherwise prints its arguments and `TILES_RUN`.
fn stub_pi(root: &Path) {
    let pi = root.join("path/pi");
    std::fs::write(
        &pi,
        "#!/bin/sh\n[ \"$1\" = --version ] && { echo 1.4.0; exit 0; }\necho \"pi [$*] TILES_RUN=[$TILES_RUN]\"\nexit \"${PI_EXIT:-0}\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
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

    let doctor = text(&tiles(&root, &["doctor"], None));
    assert!(
        doctor.contains(&format!(
            "\n  shell: wired for zsh through {}: ",
            zshrc.display()
        )),
        "{doctor}"
    );
    let bare = tiles(&root, &["shell"], None);
    assert_eq!(bare.status.code(), Some(2));
    assert_eq!(text(&bare), "tiles: usage: tiles shell off [--dry-run]\n");

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

/// A machine wired by `tiles setup --yes` run as `bin/tiles`, a link to the
/// binary under test, from a `PATH` that names it first; a fish config
/// directory makes setup write the fish script too.
fn wired(name: &str) -> PathBuf {
    let root = machine(name);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("home/.config/fish")).unwrap();
    std::os::unix::fs::symlink(TILES, root.join("bin/tiles")).unwrap();
    set_up(&root);
    root
}

/// Runs `tiles setup --yes` as the machine's `bin/tiles`, first on `PATH`.
fn set_up(root: &Path) {
    let out = command(root, root.join("bin/tiles").to_str().unwrap())
        .arg("setup")
        .arg("--yes")
        .env(
            "PATH",
            format!(
                "{}:{}:/usr/bin:/bin",
                root.join("bin").display(),
                root.join("path").display()
            ),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
}

/// Each shell found here with the line that reads Tiles' script for it.
fn shells(root: &Path) -> Vec<(&'static str, String)> {
    let sh = format!(". '{}' &&", root.join("config/shell.sh").display());
    let fish = format!(
        "source '{}'; and",
        root.join("home/.config/fish/conf.d/tiles.fish").display()
    );
    [
        ("/bin/zsh", &sh),
        ("/usr/bin/zsh", &sh),
        ("/bin/bash", &sh),
        ("/usr/bin/bash", &sh),
        ("/opt/homebrew/bin/fish", &fish),
        ("/usr/local/bin/fish", &fish),
        ("/usr/bin/fish", &fish),
    ]
    .into_iter()
    .filter(|(shell, _)| Path::new(shell).exists())
    .map(|(shell, read)| (shell, read.clone()))
    .collect()
}

/// Types `pi 'a b'` in `shell` after `read`, with standard input and output
/// a pseudo-terminal and `path` as `PATH`, and returns all it wrote.
fn at_prompt(root: &Path, shell: &str, read: &str, path: &str, run: Option<&str>) -> String {
    let (mut pty, pts) = pty_process::blocking::open().unwrap();
    pty.resize(pty_process::Size::new(24, 80)).unwrap();
    let mut c = pty_process::blocking::Command::new(shell);
    if shell.ends_with("fish") {
        c = c.arg("--no-config");
    }
    c = c
        .arg("-c")
        .arg(format!("{read} pi 'a b'"))
        .env_clear()
        .env("HOME", root.join("home"))
        .env("SHELL", "/bin/zsh")
        .env("PATH", path)
        .env("TILES_CONFIG_DIR", root.join("config"))
        .env("TILES_STATE_DIR", root.join("config"))
        .env("TILES_CACHE_DIR", root.join("cache"))
        .env("TILES_UPDATE_CHECK", "off")
        .env("TERM_PROGRAM", "WarpTerminal")
        .env("TERM", "xterm-256color")
        .current_dir(root);
    if let Some(run) = run {
        c = c.env("TILES_RUN", run);
    }
    let mut child = c.spawn(pts).unwrap();
    let mut out = Vec::new();
    let mut buf = vec![0u8; 65536];
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match pty.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
        }
    }
    let _ = child.wait();
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn a_harness_typed_in_a_terminal_starts_through_the_tiles_that_ran_setup() {
    let root = wired("typed");
    let bare = "/usr/bin:/bin".to_string();
    let path = format!("{}:{bare}", root.join("path").display());
    let found = shells(&root);
    assert!(!found.is_empty(), "no zsh, bash or fish here");
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("pi [-e "),
            "{shell}, no tiles on PATH: {said}"
        );
        assert!(
            said.contains("/pi/index.ts a b] TILES_RUN=[1]"),
            "{shell}, no tiles on PATH: {said}"
        );
    }
    let old = root.join("old");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(
        old.join("tiles"),
        "#!/bin/sh\necho \"old tiles [$*]\"\nexit 2\n",
    )
    .unwrap();
    std::fs::set_permissions(old.join("tiles"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let ahead = format!("{}:{path}", old.display());
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &ahead, None);
        assert!(
            !said.contains("old tiles"),
            "{shell}, older copy ahead: {said}"
        );
        assert!(
            said.contains("/pi/index.ts a b] TILES_RUN=[1]"),
            "{shell}, older copy ahead: {said}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_harness_typed_in_a_terminal_starts_plain_inside_tiles_run_or_once_its_tiles_is_gone() {
    let root = wired("typed-plain");
    let path = format!("{}:/usr/bin:/bin", root.join("path").display());
    let found = shells(&root);
    assert!(!found.is_empty(), "no zsh, bash or fish here");
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, Some("1"));
        assert!(
            said.contains("pi [a b] TILES_RUN=[1]"),
            "{shell}, inside tiles run: {said}"
        );
    }
    std::fs::remove_file(root.join("bin/tiles")).unwrap();
    let doctor = text(
        &command(&root, TILES)
            .arg("doctor")
            .stdin(Stdio::null())
            .output()
            .unwrap(),
    );
    assert!(
        doctor.contains(&format!(
            "but {} is missing, so pi typed in a new terminal starts plain; run tiles setup",
            root.join("bin/tiles").display()
        )),
        "{doctor}"
    );
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("pi [a b] TILES_RUN=[]"),
            "{shell}, tiles gone: {said}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_harness_missing_at_setup_gets_its_function_once_setup_runs_again() {
    let root = machine("later");
    std::fs::remove_file(root.join("path/pi")).unwrap();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("home/.config/fish")).unwrap();
    std::os::unix::fs::symlink(TILES, root.join("bin/tiles")).unwrap();
    set_up(&root);
    stub_pi(&root);
    let path = format!("{}:/usr/bin:/bin", root.join("path").display());
    let found = shells(&root);
    assert!(!found.is_empty(), "no zsh, bash or fish here");
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("pi [a b] TILES_RUN=[]"),
            "{shell}, pi installed after setup: {said}"
        );
    }
    set_up(&root);
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("/pi/index.ts a b] TILES_RUN=[1]"),
            "{shell}, after setup again: {said}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pi_that_is_another_program_gets_no_function_and_keeps_its_name() {
    let root = machine("digits");
    std::fs::write(
        root.join("path/pi"),
        "#!/bin/sh\ncase \"$1\" in --version) echo 'pi (cln) 1.3' ;; *) echo \"digits [$*] TILES_RUN=[$TILES_RUN]\" ;; esac\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("home/.config/fish")).unwrap();
    std::os::unix::fs::symlink(TILES, root.join("bin/tiles")).unwrap();
    set_up(&root);
    let path = format!("{}:/usr/bin:/bin", root.join("path").display());
    let found = shells(&root);
    assert!(!found.is_empty(), "no zsh, bash or fish here");
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("digits [a b] TILES_RUN=[]"),
            "{shell}, another pi: {said}"
        );
    }
    let doctor = text(
        &command(&root, TILES)
            .arg("doctor")
            .stdin(Stdio::null())
            .output()
            .unwrap(),
    );
    assert!(
        doctor.contains(
            "; the pi on PATH did not answer --version as Pi, so tiles setup does not wire pi"
        ),
        "{doctor}"
    );
    stub_pi(&root);
    set_up(&root);
    for (shell, read) in &found {
        let said = at_prompt(&root, shell, read, &path, None);
        assert!(
            said.contains("/pi/index.ts a b] TILES_RUN=[1]"),
            "{shell}, Pi: {said}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
