//! Which `OpenCode` command line is on `PATH`, read from its own `--help`
//! and `--version` (ADR-029). sst/opencode's CLI starts a server for each
//! TUI, and that server reads `OPENCODE_CONFIG_CONTENT`. The launcher build
//! (`opencode v2.0.32`) attaches its TUI to a background service that
//! already runs with its own configuration, so the session's Tiles server
//! reaches the model only through `--standalone`,
//! which starts a private server.

use std::io::{Read as _, Seek as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// How long `opencode --help` or `--version` may take. sst/opencode 1.19.3
/// answers in 0.8 s on the 2017 development Mac, the launcher in 0.3 s.
const PATIENCE: Duration = Duration::from_secs(5);

/// The most output read from one answer.
const MAX_ANSWER_BYTES: u64 = 64 * 1024;

/// How many names [`answer_file`] tries before giving up: a name can be
/// taken by a file or symlink someone else left in the directory.
const NAMES: u32 = 8;

/// Answer files opened by this process, numbered.
static ASKED: AtomicU32 = AtomicU32::new(0);

/// The `OpenCode` versions Tiles was proved on, per command line.
pub const PROVED_PUBLIC: &str = "1.19.3";
/// See [`PROVED_PUBLIC`]: the launcher, with `--standalone`, on 2026-10-19.
pub const PROVED_LAUNCHER: &str = "2.0.32";

/// The flag that gives the launcher's TUI a private server.
pub const STANDALONE: &str = "--standalone";

/// An `OpenCode` command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    /// sst/opencode's own: each TUI starts its own server.
    Public,
    /// A launcher whose TUI attaches to a background service unless given
    /// `--standalone`.
    Launcher,
}

/// The command line a `--help` answer describes: the launcher lists
/// `--standalone` ("Run with a private server instead of the background
/// service"); sst/opencode 1.19.3 has no such flag.
#[must_use]
pub fn cli_of(help: &str) -> Cli {
    if help
        .split(|c: char| c.is_whitespace() || c == ',')
        .any(|w| w == STANDALONE)
    {
        Cli::Launcher
    } else {
        Cli::Public
    }
}

/// The command line `program` is, from its `--help`; `None` when it does not
/// answer.
#[must_use]
pub fn read_cli(program: &Path) -> Option<Cli> {
    ask(program, "--help").map(|h| cli_of(&h))
}

/// The version in a `--version` answer: `1.19.3` from sst/opencode,
/// `2.0.32` from the launcher's `opencode v2.0.32`. Only digits, letters,
/// dots, `+` and `-` are kept, at most 32 of them.
#[must_use]
pub fn version_of(answer: &str) -> Option<String> {
    answer.split_whitespace().find_map(|w| {
        let w = w.strip_prefix('v').unwrap_or(w);
        let ok = w.len() <= 32
            && w.starts_with(|c: char| c.is_ascii_digit())
            && w.contains('.')
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'));
        ok.then(|| w.to_string())
    })
}

/// What `tiles run`, `tiles check` and `tiles doctor` know of `OpenCode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The command line, `None` when `--help` did not answer.
    pub cli: Option<Cli>,
    /// Its version, `None` when `--version` did not answer with one.
    pub version: Option<String>,
}

impl Found {
    /// Asks `program` for its `--help` and `--version`.
    #[must_use]
    pub fn read(program: &Path) -> Self {
        Self {
            cli: read_cli(program),
            version: ask(program, "--version").and_then(|v| version_of(&v)),
        }
    }

    /// One line on the version and on which server a `tiles run opencode`
    /// session's model calls, and whether Tiles was proved on that version.
    #[must_use]
    pub fn describe(&self) -> (bool, String) {
        let version = self.version.as_deref().unwrap_or("of unknown version");
        let (cli, proved, server) = match self.cli {
            None => {
                return (
                    false,
                    format!(
                        "OpenCode {version}, whose --help did not answer, so tiles run cannot tell whether it needs {STANDALONE}"
                    ),
                );
            }
            Some(Cli::Public) => (
                "sst/opencode's CLI",
                PROVED_PUBLIC,
                "each TUI starts its own server, which loads the session's tiles server from OPENCODE_CONFIG_CONTENT",
            ),
            Some(Cli::Launcher) => (
                "a launcher with a background service",
                PROVED_LAUNCHER,
                "tiles run passes --standalone for a private server, which loads the session's tiles server",
            ),
        };
        let major = |v: &str| v.split('.').next().map(str::to_string);
        let ok = self.version.as_deref().and_then(major) == major(proved);
        let verdict = if ok {
            format!("proved on {proved}")
        } else {
            format!("not the major version Tiles was proved on, {proved}")
        };
        (
            ok,
            format!("OpenCode {version}, {cli}: {server}; {verdict}"),
        )
    }
}

/// `program flag`'s answer on stdout and stderr together, or `None` when
/// it cannot start, fails or takes longer than [`PATIENCE`]. The answer goes
/// to an unlinked file, so a full pipe never stalls it, and stdin is closed,
/// so it never reads the terminal.
pub(crate) fn ask(program: &Path, flag: &str) -> Option<String> {
    ask_in(&std::env::temp_dir(), program, &[flag], PATIENCE)
}

/// [`ask`] with several arguments and `patience` in place of [`PATIENCE`].
pub(crate) fn ask_with(program: &Path, args: &[&str], patience: Duration) -> Option<String> {
    ask_in(&std::env::temp_dir(), program, args, patience)
}

/// [`ask_with`] with its answer file in `dir`.
fn ask_in(dir: &Path, program: &Path, args: &[&str], patience: Duration) -> Option<String> {
    let mut out = answer_file(dir)?;
    let start = Instant::now();
    let mut child = loop {
        let spawned = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(out.try_clone().ok()?)
            .stderr(out.try_clone().ok()?)
            .spawn();
        match spawned {
            Ok(child) => break child,
            // Linux will not run a file still open for writing, and a
            // program just written (`tiles update`'s new binary) stays open
            // in any child another thread forked meanwhile, until it execs.
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && start.elapsed() < patience =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < patience => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    out.rewind().ok()?;
    let mut bytes = Vec::new();
    out.take(MAX_ANSWER_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A new, unlinked file in `dir` for one answer. Each name is used once
/// per process, and a name that already exists, a symlink included, is
/// skipped rather than opened.
fn answer_file(dir: &Path) -> Option<std::fs::File> {
    for _ in 0..NAMES {
        let path = dir.join(format!(
            ".tiles-opencode-{}-{}",
            std::process::id(),
            ASKED.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                let _ = std::fs::remove_file(&path);
                return Some(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Cli, Found, cli_of, version_of};

    /// The launcher's `--help`, 2026-10-19.
    const LAUNCHER_HELP: &str = "DESCRIPTION
  OpenCode command line interface

USAGE
  opencode <subcommand> [flags] [<directory>]

FLAGS
  --standalone           Run with a private server instead of the background service
  --server string        Connect to a server URL instead of the background service
  --auto                 Auto-approve permissions that are not explicitly denied
";

    /// sst/opencode 1.19.3's `--help`, its options in part.
    const PUBLIC_HELP: &str = "Commands:
  opencode [project]           start opencode tui                [default]
  opencode attach <url>        attach to a running opencode server
Options:
      --port          port to listen on              [number] [default: 0]
      --pure          run without external plugins             [boolean]
";

    #[test]
    fn the_launcher_is_told_by_its_standalone_flag() {
        assert_eq!(cli_of(LAUNCHER_HELP), Cli::Launcher);
        assert_eq!(cli_of(PUBLIC_HELP), Cli::Public);
        assert_eq!(
            cli_of("--standalone-ish is another flag"),
            Cli::Public,
            "the flag itself, not a word that starts with it"
        );
    }

    #[test]
    fn the_version_is_read_from_either_answer() {
        assert_eq!(version_of("1.19.3\n").as_deref(), Some("1.19.3"));
        assert_eq!(version_of("opencode v2.0.32").as_deref(), Some("2.0.32"));
        assert_eq!(version_of("opencode dev"), None);
        assert_eq!(version_of("\u{1b}]0;x\u{7} 1.2.3\u{1b}[2J"), None);
        assert_eq!(version_of("1.2.3\u{1b}[2J v4.5"), Some("4.5".into()));
    }

    #[test]
    fn the_line_names_the_server_and_the_proved_version() {
        let found = |cli, version: Option<&str>| Found {
            cli,
            version: version.map(str::to_string),
        };
        assert_eq!(
            found(Some(Cli::Launcher), Some("2.0.32")).describe(),
            (
                true,
                "OpenCode 2.0.32, a launcher with a background service: tiles run passes --standalone for a private server, which loads the session's tiles server; proved on 2.0.32".into()
            )
        );
        assert_eq!(
            found(Some(Cli::Public), Some("1.20.0")).describe(),
            (
                true,
                "OpenCode 1.20.0, sst/opencode's CLI: each TUI starts its own server, which loads the session's tiles server from OPENCODE_CONFIG_CONTENT; proved on 1.19.3".into()
            )
        );
        let (ok, said) = found(Some(Cli::Public), Some("2.1.0")).describe();
        assert!(!ok);
        assert!(
            said.ends_with("not the major version Tiles was proved on, 1.19.3"),
            "{said}"
        );
        let (ok, said) = found(Some(Cli::Launcher), None).describe();
        assert!(!ok && said.starts_with("OpenCode of unknown version, a launcher"));
        let (ok, said) = found(None, Some("2.0.32")).describe();
        assert!(!ok && said.contains("cannot tell whether it needs --standalone"));
    }

    #[test]
    fn a_planted_name_is_never_opened() {
        use std::sync::atomic::Ordering;
        let dir =
            std::env::temp_dir().join(format!("tiles-opencode-planted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let first = super::ASKED.load(Ordering::Relaxed);
        for n in first..first + super::NAMES + 16 {
            std::os::unix::fs::symlink(
                &victim,
                dir.join(format!(".tiles-opencode-{}-{n}", std::process::id())),
            )
            .unwrap();
        }
        assert_eq!(
            super::ask_in(
                &dir,
                std::path::Path::new("/bin/echo"),
                &["--help"],
                super::PATIENCE
            ),
            None
        );
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_answer_is_read_and_a_missing_program_gives_none() {
        let sh = std::path::Path::new("/bin/echo");
        assert_eq!(
            super::ask(sh, "--standalone").as_deref(),
            Some("--standalone\n")
        );
        assert_eq!(
            super::ask(std::path::Path::new("/nonexistent/opencode"), "--help"),
            None
        );
        assert_eq!(
            super::ask(std::path::Path::new("/usr/bin/false"), "--help"),
            None
        );
    }

    #[test]
    fn a_program_still_open_for_writing_is_asked_once_it_closes() {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let dir = std::env::temp_dir().join(format!("tiles-ask-busy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("tiles");
        let mut writing = std::fs::File::options()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&program)
            .unwrap();
        writing
            .write_all(b"#!/bin/sh\necho \"tiles 9.9.9\"\n")
            .unwrap();
        let closer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(writing);
        });
        assert_eq!(
            super::ask(&program, "--version").as_deref(),
            Some("tiles 9.9.9\n")
        );
        closer.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
