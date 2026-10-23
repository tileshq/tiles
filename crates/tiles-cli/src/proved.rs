//! The harness versions Tiles was proved on, and how an installed one
//! compares (ADR-032). `tiles run` names a version Tiles was not proved on
//! before the harness starts, `tiles check` fails its line, and `tiles
//! doctor` prints each version beside its harness. Pi, Claude Code and Codex
//! are compared by major and minor version; `OpenCode` keeps ADR-029's rule,
//! the major version, for each of its two command lines.
//!
//! A maintainer re-proves Tiles on each new harness release and adds it to
//! the row here, with the record that proved it. An installed version is
//! proved when it matches any version in its row; messages name the newest.

use std::path::Path;

use crate::harness::Harness;
use crate::opencode::{self, Cli};

/// One harness version Tiles was proved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The harness.
    pub harness: Harness,
    /// Which `OpenCode` command line, for `OpenCode`'s two rows.
    pub cli: Option<Cli>,
    /// Every version it was proved on, oldest first.
    pub versions: &'static [&'static str],
    /// The record that proved it.
    pub record: &'static str,
}

/// The versions Tiles was proved on, one row per harness and `OpenCode`'s
/// two as ADR-029 names them.
pub const PROVED: [Row; 5] = [
    Row {
        harness: Harness::Pi,
        cli: None,
        versions: &["1.0.0", "1.2.0", "1.3.0", "1.4.0"],
        record: "ADR-032",
    },
    Row {
        harness: Harness::Claude,
        cli: None,
        versions: &["2.1.308"],
        record: "ADR-032",
    },
    Row {
        harness: Harness::Codex,
        cli: None,
        versions: &["0.163.0"],
        record: "ADR-011",
    },
    Row {
        harness: Harness::OpenCode,
        cli: Some(Cli::Public),
        versions: &[opencode::PROVED_PUBLIC],
        record: "ADR-029",
    },
    Row {
        harness: Harness::OpenCode,
        cli: Some(Cli::Launcher),
        versions: &[opencode::PROVED_LAUNCHER],
        record: "ADR-029",
    },
];

impl Row {
    /// The newest version it was proved on, the one messages name.
    #[must_use]
    pub fn newest(&self) -> &'static str {
        self.versions.last().copied().unwrap_or_default()
    }
}

/// The row for `harness`; for `OpenCode`, the row for `cli`, `None` when
/// its `--help` did not say which.
#[must_use]
pub fn row(harness: Harness, cli: Option<Cli>) -> Option<&'static Row> {
    PROVED
        .iter()
        .find(|r| r.harness == harness && (harness != Harness::OpenCode || r.cli == cli))
}

/// The harness's name as its makers write it.
#[must_use]
pub fn title(harness: Harness) -> &'static str {
    match harness {
        Harness::Pi => "Pi",
        Harness::OpenCode => "OpenCode",
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
    }
}

/// The version `program --version` names: `2.1.308 (Claude Code)`,
/// `1.4.0` and `codex-cli 0.163.0` all name one. Read the way
/// [`opencode`] reads `OpenCode`'s: stdin closed, the answer in an unlinked
/// file, at most 5 s.
#[must_use]
pub fn read(program: &Path) -> Option<String> {
    opencode::ask(program, "--version").and_then(|a| opencode::version_of(&a))
}

/// Whether `installed` is a version `row` was proved on, by the parts that
/// harness compares.
#[must_use]
pub fn matches(row: &Row, installed: &str) -> bool {
    let parts = if row.harness == Harness::OpenCode {
        1
    } else {
        2
    };
    let head = |v: &str| {
        v.split(['.', '-', '+'])
            .take(parts)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    row.versions.iter().any(|v| {
        let want = head(v);
        want.len() == parts && head(installed) == want
    })
}

/// The line `tiles run` prints before the harness starts when the
/// installed version is not one Tiles was proved on; `None` when it is, or
/// when the version could not be read.
#[must_use]
pub fn run_line(harness: Harness, cli: Option<Cli>, installed: Option<&str>) -> Option<String> {
    let row = row(harness, cli)?;
    let installed = installed?;
    (!matches(row, installed)).then(|| {
        format!(
            "Tiles was proved on {} {}, and this is {installed}; if a card misdraws, run tiles check",
            title(harness),
            row.newest()
        )
    })
}

/// `tiles check`'s version line for Pi, Claude Code or Codex: passes on
/// a proved major and minor version and when the version cannot be read,
/// which it says. `OpenCode`'s is the `opencode server:` line (ADR-029).
#[must_use]
pub fn check_line(harness: Harness, installed: Option<&str>) -> Option<(bool, String)> {
    let row = row(harness, None)?;
    let name = harness.name();
    let proved = row.newest();
    Some(match installed {
        None => (
            true,
            format!(
                "{name} version: {name} --version could not be read; Tiles was proved on {proved}"
            ),
        ),
        Some(v) if matches(row, v) => (
            true,
            format!(
                "{name} version: {} {v}, proved on {proved} ({})",
                title(harness),
                row.record
            ),
        ),
        Some(v) => (
            false,
            format!(
                "{name} version: {} {v}, not the major and minor version Tiles was proved on, {proved} ({})",
                title(harness),
                row.record
            ),
        ),
    })
}

/// What `tiles doctor` prints beside a harness's row.
#[must_use]
pub fn doctor_note(harness: Harness, cli: Option<Cli>, installed: Option<&str>) -> String {
    let version = installed.map_or_else(|| "version unknown".to_string(), str::to_string);
    match row(harness, cli) {
        Some(row) => format!("{version} (proved on {})", row.newest()),
        None => version,
    }
}

#[cfg(test)]
mod tests {
    use super::{PROVED, check_line, doctor_note, matches, row, run_line};
    use crate::harness::Harness;
    use crate::opencode::Cli;

    #[test]
    fn the_table_holds_each_harness_once_and_opencodes_two_lines() {
        for h in [Harness::Pi, Harness::Claude, Harness::Codex] {
            assert_eq!(PROVED.iter().filter(|r| r.harness == h).count(), 1);
            assert_eq!(row(h, None).unwrap().harness, h);
        }
        assert_eq!(
            row(Harness::Pi, None).unwrap().versions,
            ["1.0.0", "1.2.0", "1.3.0", "1.4.0"]
        );
        assert_eq!(row(Harness::Pi, None).unwrap().newest(), "1.4.0");
        assert_eq!(row(Harness::Claude, None).unwrap().versions, ["2.1.308"]);
        assert_eq!(row(Harness::Codex, None).unwrap().versions, ["0.163.0"]);
        assert_eq!(
            row(Harness::OpenCode, Some(Cli::Public)).unwrap().versions,
            [crate::opencode::PROVED_PUBLIC]
        );
        assert_eq!(
            row(Harness::OpenCode, Some(Cli::Launcher))
                .unwrap()
                .versions,
            [crate::opencode::PROVED_LAUNCHER]
        );
        assert_eq!(row(Harness::OpenCode, None), None);
    }

    #[test]
    fn pi_claude_and_codex_compare_major_and_minor() {
        let claude = row(Harness::Claude, None).unwrap();
        assert!(matches(claude, "2.1.308"));
        assert!(matches(claude, "2.1.307"));
        assert!(matches(claude, "2.1.306"));
        assert!(matches(claude, "2.1.305"));
        assert!(matches(claude, "2.1.304"));
        assert!(matches(claude, "2.1.303"));
        assert!(matches(claude, "2.1.400"));
        assert!(!matches(claude, "2.2.0"));
        assert!(!matches(claude, "3.1.303"));
        assert!(!matches(claude, "2"));
        let codex = row(Harness::Codex, None).unwrap();
        assert!(matches(codex, "0.163.2"));
        assert!(!matches(codex, "0.164.0"));
        assert!(!matches(codex, "0.16.3"));
        let pi = row(Harness::Pi, None).unwrap();
        assert!(matches(pi, "1.4.0"));
        assert!(matches(pi, "1.4.3"));
        assert!(matches(pi, "1.3.0"));
        assert!(matches(pi, "1.3.7"));
        assert!(matches(pi, "1.2.0"));
        assert!(matches(pi, "1.0.4"));
        assert!(!matches(pi, "1.1.0"));
        assert!(!matches(pi, "1.5.0"));
        assert!(!matches(pi, "2.0.0"));
        assert!(matches(pi, "1.3.0-beta.2"));
    }

    #[test]
    fn opencode_keeps_its_major_version_rule() {
        let public = row(Harness::OpenCode, Some(Cli::Public)).unwrap();
        assert!(matches(public, "1.20.0"));
        assert!(!matches(public, "2.0.0"));
        let launcher = row(Harness::OpenCode, Some(Cli::Launcher)).unwrap();
        assert!(matches(launcher, "2.1.0"));
        assert!(!matches(launcher, "1.19.3"));
    }

    #[test]
    fn run_names_the_proved_version_only_when_it_differs() {
        assert_eq!(run_line(Harness::Claude, None, Some("2.1.310")), None);
        assert_eq!(
            run_line(Harness::Claude, None, Some("2.2.0")).as_deref(),
            Some(
                "Tiles was proved on Claude Code 2.1.308, and this is 2.2.0; if a card misdraws, run tiles check"
            )
        );
        assert_eq!(
            run_line(Harness::Codex, None, Some("0.170.1")).as_deref(),
            Some(
                "Tiles was proved on Codex 0.163.0, and this is 0.170.1; if a card misdraws, run tiles check"
            )
        );
        assert!(
            run_line(Harness::Pi, None, Some("2.0.0"))
                .unwrap()
                .starts_with("Tiles was proved on Pi 1.4.0, and this is 2.0.0")
        );
        assert_eq!(run_line(Harness::Pi, None, Some("1.3.2")), None);
        assert_eq!(run_line(Harness::Pi, None, Some("1.2.0")), None);
        assert_eq!(run_line(Harness::Pi, None, Some("1.4.0")), None);
        assert_eq!(
            run_line(Harness::Pi, None, Some("1.5.0")).as_deref(),
            Some(
                "Tiles was proved on Pi 1.4.0, and this is 1.5.0; if a card misdraws, run tiles check"
            )
        );
        assert_eq!(run_line(Harness::Pi, None, None), None, "unread: silent");
        assert_eq!(
            run_line(Harness::OpenCode, Some(Cli::Public), Some("1.20.0")),
            None
        );
        assert!(
            run_line(Harness::OpenCode, Some(Cli::Launcher), Some("3.0.0"))
                .unwrap()
                .starts_with("Tiles was proved on OpenCode 2.0.32, and this is 3.0.0")
        );
        assert_eq!(run_line(Harness::OpenCode, None, Some("9.0.0")), None);
    }

    #[test]
    fn check_fails_another_minor_and_only_says_an_unread_version() {
        assert_eq!(
            check_line(Harness::Claude, Some("2.1.308")),
            Some((
                true,
                "claude version: Claude Code 2.1.308, proved on 2.1.308 (ADR-032)".into()
            ))
        );
        assert_eq!(
            check_line(Harness::Codex, Some("0.164.0")),
            Some((
                false,
                "codex version: Codex 0.164.0, not the major and minor version Tiles was proved on, 0.163.0 (ADR-011)".into()
            ))
        );
        assert_eq!(
            check_line(Harness::Pi, Some("1.2.0")),
            Some((
                true,
                "pi version: Pi 1.2.0, proved on 1.4.0 (ADR-032)".into()
            ))
        );
        assert_eq!(
            check_line(Harness::Pi, Some("1.4.0")),
            Some((
                true,
                "pi version: Pi 1.4.0, proved on 1.4.0 (ADR-032)".into()
            ))
        );
        assert_eq!(
            check_line(Harness::Pi, Some("1.5.0")),
            Some((
                false,
                "pi version: Pi 1.5.0, not the major and minor version Tiles was proved on, 1.4.0 (ADR-032)".into()
            ))
        );
        assert_eq!(
            check_line(Harness::Pi, None),
            Some((
                true,
                "pi version: pi --version could not be read; Tiles was proved on 1.4.0".into()
            ))
        );
        assert_eq!(check_line(Harness::OpenCode, Some("1.19.3")), None);
    }

    #[test]
    fn doctor_prints_the_version_beside_the_proved_one() {
        assert_eq!(
            doctor_note(Harness::Pi, None, Some("1.2.0")),
            "1.2.0 (proved on 1.4.0)"
        );
        assert_eq!(
            doctor_note(Harness::Codex, None, None),
            "version unknown (proved on 0.163.0)"
        );
        assert_eq!(
            doctor_note(Harness::OpenCode, Some(Cli::Launcher), Some("2.0.32")),
            "2.0.32 (proved on 2.0.32)"
        );
        assert_eq!(
            doctor_note(Harness::OpenCode, None, None),
            "version unknown"
        );
    }

    #[test]
    fn each_harness_answer_is_read_through_the_opencode_reader() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("tiles-proved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, answer, want) in [
            ("claude", "2.1.308 (Claude Code)", Some("2.1.308")),
            ("pi", "1.4.0", Some("1.4.0")),
            ("codex", "codex-cli 0.163.0", Some("0.163.0")),
            ("odd", "\u{1b}]0;x\u{7}dev build", None),
        ] {
            let program = dir.join(name);
            std::fs::write(&program, format!("#!/bin/sh\nprintf '%s\\n' '{answer}'\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(super::read(&program).as_deref(), want, "{name}");
        }
        assert_eq!(
            super::read(std::path::Path::new("/nonexistent/claude")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
