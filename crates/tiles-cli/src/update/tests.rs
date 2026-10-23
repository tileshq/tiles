//! `tiles update` against a fake releases directory and a fake `gh`, never
//! the real repository or the network, and the daily check's gate, state
//! and notice (ADR-032).

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{
    Args, Context, RELEASES_REPO, State, Version, archive_version, doctor_line, due, expected_sum,
    from_package, latest_from_metadata, metadata_url, notice, parse, read_state, refresh, repo,
    sha256, state_path, update,
};

const TARGET: &str = "aarch64-apple-darwin";

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tiles-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// A release of `version` in `dir`: its archive, whose `tiles` answers
/// `--version` with `says`, and `SHA256SUMS`.
fn release(dir: &Path, version: &str, says: &str) -> (PathBuf, PathBuf) {
    let name = format!("tiles-v{version}-{TARGET}");
    let stage = dir.join(format!("stage-{version}"));
    std::fs::create_dir_all(stage.join(&name)).unwrap();
    script(
        &stage.join(&name).join("tiles"),
        &format!("echo \"tiles {says} (intent schema 1)\""),
    );
    let archive = dir.join(format!("{name}.tar.gz"));
    let made = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg(&name)
        .status()
        .unwrap();
    assert!(made.success());
    let sums = dir.join("SHA256SUMS");
    std::fs::write(
        &sums,
        format!(
            "{}  {name}.tar.gz\n",
            hex(&sha256(&std::fs::read(&archive).unwrap()))
        ),
    )
    .unwrap();
    std::fs::remove_dir_all(&stage).unwrap();
    (archive, sums)
}

/// A fake `gh` that logs its arguments, answers `auth status` with
/// `auth_exit`, names `latest` as the latest release and copies every file
/// in `releases` into a download's `--dir`.
fn fake_gh(dir: &Path, releases: &Path, latest: &str, auth_exit: u8) -> PathBuf {
    let gh = dir.join("gh");
    script(
        &gh,
        &format!(
            r#"echo "$*" >> "{log}"
case "$1 $2" in
  "auth status") exit {auth_exit} ;;
  "release view") echo {latest} ;;
  "release download")
    while [ $# -gt 0 ]; do [ "$1" = --dir ] && cp "{releases}/"* "$2/"; shift; done ;;
esac"#,
            log = dir.join("gh.log").display(),
            releases = releases.display(),
        ),
    );
    gh
}

fn gh_log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("gh.log")).unwrap_or_default()
}

struct Machine {
    root: PathBuf,
    vars: HashMap<String, String>,
    gh: Option<PathBuf>,
    current: &'static str,
}

impl Machine {
    /// A machine with `tiles` 0.1.4 in `bin/` and a release v9.9.9 in
    /// `releases/`, offered by a signed-in fake `gh`.
    fn new(name: &str) -> Self {
        let root = scratch(name);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("work")).unwrap();
        std::fs::create_dir_all(root.join("releases")).unwrap();
        script(
            &root.join("bin/tiles"),
            "echo \"tiles 0.1.4 (intent schema 1)\"",
        );
        release(&root.join("releases"), "9.9.9", "9.9.9");
        let gh = fake_gh(&root, &root.join("releases"), "v9.9.9", 0);
        let mut vars = HashMap::new();
        vars.insert("PATH".to_string(), root.join("bin").display().to_string());
        Self {
            root,
            vars,
            gh: Some(gh),
            current: "0.1.4",
        }
    }

    fn dest(&self) -> PathBuf {
        self.root.join("bin/tiles")
    }

    fn run(&self, args: &Args) -> Result<Vec<String>, String> {
        let env = |k: &str| self.vars.get(k).cloned();
        update(
            args,
            &Context {
                env: &env,
                gh: self.gh.clone(),
                current: self.current,
                target: TARGET,
                dest: self.dest(),
                running: Some(self.dest()),
                work: self.root.join("work"),
                latest: &|_| Err("GitHub answered 404".into()),
            },
        )
    }

    /// Nothing but `tiles` in `bin/` and nothing left in `work/`.
    fn assert_clean(&self) {
        let names = |d: &str| -> Vec<String> {
            std::fs::read_dir(self.root.join(d))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(names("bin"), ["tiles"]);
        assert!(names("work").is_empty(), "{:?}", names("work"));
    }

    fn installed_says(&self) -> String {
        String::from_utf8(
            Command::new(self.dest())
                .arg("--version")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    }
}

#[test]
fn the_releases_location_is_one_constant_with_an_override() {
    assert_eq!(RELEASES_REPO, "tileshq/tiles");
    assert_eq!(repo(&|_| None).unwrap(), RELEASES_REPO);
    let set = |v: &'static str| move |k: &str| (k == "TILES_RELEASES_REPO").then(|| v.to_string());
    assert_eq!(repo(&set("acme/tiles")).unwrap(), "acme/tiles");
    for bad in [
        "acme",
        "acme/",
        "/tiles",
        "a/b/c",
        "../x",
        "a/..",
        "a b/c",
        "acme/t\u{1b}[2J",
    ] {
        assert!(repo(&set(bad)).is_err(), "{bad:?}");
    }
    assert_eq!(
        metadata_url("tileshq/tiles"),
        "https://api.github.com/repos/tileshq/tiles/releases/latest"
    );
}

#[test]
fn versions_tags_and_archive_names_parse_strictly() {
    assert_eq!(Version::parse("v0.1.4"), Some(Version(0, 1, 4)));
    assert_eq!(Version::parse("0.10.0"), Some(Version(0, 10, 0)));
    assert!(Version(0, 10, 0) > Version(0, 9, 9));
    for bad in [
        "",
        "v",
        "1.2",
        "1.2.3.4",
        "1.2.x",
        "1..3",
        "v1.2.3-rc1",
        "1.2.3 ",
        "1234567890.0.0",
    ] {
        assert_eq!(Version::parse(bad), None, "{bad:?}");
    }
    let args = |a: &[&str]| parse(&a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
    assert_eq!(
        args(&["v0.1.5", "--allow-older"]).unwrap(),
        Args {
            check: false,
            allow_older: true,
            tag: Some(Version(0, 1, 5))
        }
    );
    assert!(args(&["--check"]).unwrap().check);
    assert!(args(&["0.1.5"]).is_err(), "a tag carries its v");
    assert!(args(&["v0.1.5", "v0.1.6"]).is_err());
    assert!(args(&["--check", "v0.1.5"]).is_err());
    assert!(args(&["--force"]).is_err());
    assert_eq!(
        archive_version("tiles-v0.1.5-aarch64-apple-darwin.tar.gz", TARGET),
        Some(Version(0, 1, 5))
    );
    assert_eq!(
        archive_version("tiles-v0.1.5-x86_64-apple-darwin.tar.gz", TARGET),
        None
    );
    assert_eq!(
        archive_version("tiles-v0.1.5-aarch64-apple-darwin.zip", TARGET),
        None
    );
}

#[test]
fn sums_are_read_by_name_and_a_conflict_is_refused() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let name = "tiles-v1.0.0-aarch64-apple-darwin.tar.gz";
    assert_eq!(
        expected_sum(&format!("{b}  other.tar.gz\n{a}  {name}\n"), name).unwrap(),
        [0xaa; 32]
    );
    assert_eq!(
        expected_sum(&format!("{a} *{name}\n"), name).unwrap(),
        [0xaa; 32]
    );
    assert!(
        expected_sum(&format!("{b}  other.tar.gz\n"), name)
            .unwrap_err()
            .contains("no line for")
    );
    assert!(
        expected_sum(&format!("{a}  {name}\n{b}  {name}\n"), name)
            .unwrap_err()
            .contains("two sums")
    );
    assert!(expected_sum(&format!("xyz  {name}\n"), name).is_err());
}

#[test]
fn gh_downloads_the_latest_release_and_it_replaces_tiles_in_one_rename() {
    let m = Machine::new("gh");
    let mut old = std::fs::File::open(m.dest()).unwrap();
    let lines = m.run(&Args::default()).unwrap();
    assert_eq!(
        lines,
        [format!(
            "updated tiles from 0.1.4 to 9.9.9 at {}",
            m.dest().display()
        )]
    );
    assert_eq!(m.installed_says(), "tiles 9.9.9 (intent schema 1)\n");
    let mut was = String::new();
    std::io::Read::read_to_string(&mut old, &mut was).unwrap();
    assert!(
        was.contains("0.1.4"),
        "the old file is untouched, only renamed over: {was}"
    );
    let log = gh_log(&m.root);
    assert!(log.contains("auth status\n"), "{log}");
    assert!(
        log.contains("release view --repo tileshq/tiles --json tagName --jq .tagName\n"),
        "{log}"
    );
    assert!(
        log.contains(&format!(
            "release download v9.9.9 --repo tileshq/tiles --dir {}",
            m.root.join("work").display()
        )),
        "{log}"
    );
    assert!(
        log.contains(&format!(
            "--pattern tiles-v9.9.9-{TARGET}.tar.gz --pattern SHA256SUMS"
        )),
        "{log}"
    );
    m.assert_clean();
}

#[test]
fn a_tampered_sum_or_archive_is_refused_and_nothing_is_replaced() {
    let m = Machine::new("tampered-sum");
    let sums = m.root.join("releases/SHA256SUMS");
    let text = std::fs::read_to_string(&sums).unwrap();
    let flipped = if text.starts_with('0') { "1" } else { "0" };
    std::fs::write(&sums, format!("{flipped}{}", &text[1..])).unwrap();
    let e = m.run(&Args::default()).unwrap_err();
    assert_eq!(
        e,
        format!("tiles-v9.9.9-{TARGET}.tar.gz does not match SHA256SUMS; nothing was replaced")
    );
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
    m.assert_clean();

    let m = Machine::new("tampered-archive");
    let archive = m
        .root
        .join(format!("releases/tiles-v9.9.9-{TARGET}.tar.gz"));
    let mut bytes = std::fs::read(&archive).unwrap();
    bytes.push(0);
    std::fs::write(&archive, bytes).unwrap();
    assert!(
        m.run(&Args::default())
            .unwrap_err()
            .contains("does not match SHA256SUMS")
    );
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
    m.assert_clean();

    let m = Machine::new("no-sum");
    std::fs::write(m.root.join("releases/SHA256SUMS"), "").unwrap();
    assert!(
        m.run(&Args::default())
            .unwrap_err()
            .contains("SHA256SUMS has no line for")
    );
    m.assert_clean();
}

#[test]
fn a_binary_that_does_not_say_its_version_is_not_installed() {
    let m = Machine::new("liar");
    std::fs::remove_dir_all(m.root.join("releases")).unwrap();
    std::fs::create_dir_all(m.root.join("releases")).unwrap();
    release(&m.root.join("releases"), "9.9.9", "6.6.6");
    let e = m.run(&Args::default()).unwrap_err();
    assert_eq!(
        e,
        "the new binary's --version does not say 9.9.9; nothing was replaced"
    );
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
    m.assert_clean();
}

#[test]
fn an_older_release_needs_allow_older_and_the_same_one_is_left() {
    let mut m = Machine::new("older");
    m.current = "10.0.0";
    let e = m.run(&Args::default()).unwrap_err();
    assert_eq!(
        e,
        "v9.9.9 is older than this tiles, 10.0.0; pass --allow-older to install it"
    );
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
    let lines = m
        .run(&Args {
            allow_older: true,
            ..Args::default()
        })
        .unwrap();
    assert!(
        lines[0].starts_with("updated tiles from 10.0.0 to 9.9.9"),
        "{lines:?}"
    );
    m.current = "9.9.9";
    assert_eq!(
        m.run(&Args::default()).unwrap(),
        ["tiles 9.9.9 is already release v9.9.9; nothing to update"]
    );
    m.assert_clean();
}

#[test]
fn a_named_tag_is_downloaded_without_asking_for_the_latest() {
    let m = Machine::new("tag");
    m.run(&Args {
        tag: Some(Version(9, 9, 9)),
        ..Args::default()
    })
    .unwrap();
    let log = gh_log(&m.root);
    assert!(!log.contains("release view"), "{log}");
    assert!(log.contains("release download v9.9.9 "), "{log}");
}

#[test]
fn check_only_reports() {
    let m = Machine::new("check");
    let check = Args {
        check: true,
        ..Args::default()
    };
    assert_eq!(
        m.run(&check).unwrap(),
        ["Tiles v9.9.9 is available; run tiles update"]
    );
    assert!(!gh_log(&m.root).contains("release download"));
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
    let mut m = m;
    m.current = "9.9.9";
    assert_eq!(
        m.run(&check).unwrap(),
        ["tiles 9.9.9 is the latest release of tileshq/tiles"]
    );
    m.gh = None;
    let e = m.run(&check).unwrap_err();
    assert_eq!(
        e,
        "cannot read the latest release of tileshq/tiles: GitHub answered 404; gh is not on PATH, and GitHub answers an anonymous request only for a public repository"
    );
    m.assert_clean();
}

#[test]
fn without_gh_signed_in_the_steps_for_a_download_by_hand_are_printed() {
    let mut m = Machine::new("steps");
    m.gh = None;
    let e = m.run(&Args::default()).unwrap_err();
    assert_eq!(
        e,
        format!(
            "gh is not on PATH, so tiles update cannot download the release itself. Download it by hand:
  1. open https://github.com/tileshq/tiles/releases/latest in a browser signed in to GitHub as an account that can read tileshq/tiles
  2. download tiles-<tag>-{TARGET}.tar.gz and SHA256SUMS
  3. run TILES_ARCHIVE=~/Downloads/tiles-<tag>-{TARGET}.tar.gz TILES_SUMS=~/Downloads/SHA256SUMS tiles update
     with <tag> the release's tag, such as v0.1.5
tiles update checks the archive against SHA256SUMS and removes the quarantine flag the browser adds. Or install gh, run gh auth login, then tiles update"
        )
    );
    let mut m = Machine::new("signed-out");
    m.gh = Some(fake_gh(&m.root, &m.root.join("releases"), "v9.9.9", 1));
    let e = m
        .run(&Args {
            tag: Some(Version(9, 9, 9)),
            ..Args::default()
        })
        .unwrap_err();
    assert!(
        e.starts_with("gh is not signed in as an account that can read tileshq/tiles, so"),
        "{e}"
    );
    assert!(e.contains("releases/tag/v9.9.9 in a browser"), "{e}");
    assert!(e.contains(&format!(
        "TILES_ARCHIVE=~/Downloads/tiles-v9.9.9-{TARGET}.tar.gz "
    )));
    assert!(!gh_log(&m.root).contains("release "));
    assert_eq!(m.installed_says(), "tiles 0.1.4 (intent schema 1)\n");
}

#[test]
fn a_download_by_hand_is_checked_unquarantined_and_installed() {
    let mut m = Machine::new("by-hand");
    m.gh = None;
    let downloads = m.root.join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let (archive, sums) = release(&downloads, "9.9.9", "9.9.9");
    quarantine(&archive);
    m.vars
        .insert("TILES_ARCHIVE".into(), archive.display().to_string());
    let e = m.run(&Args::default()).unwrap_err();
    assert_eq!(e, "set TILES_SUMS to the release's SHA256SUMS file too");
    m.vars
        .insert("TILES_SUMS".into(), sums.display().to_string());
    let e = m
        .run(&Args {
            tag: Some(Version(9, 9, 8)),
            ..Args::default()
        })
        .unwrap_err();
    assert_eq!(e, "TILES_ARCHIVE holds v9.9.9, not the v9.9.8 named");
    let lines = m.run(&Args::default()).unwrap();
    assert!(
        lines[0].starts_with("updated tiles from 0.1.4 to 9.9.9 at "),
        "{lines:?}"
    );
    assert_eq!(m.installed_says(), "tiles 9.9.9 (intent schema 1)\n");
    assert!(!quarantined(&archive), "the archive's flag is removed");
    m.assert_clean();

    m.vars.insert(
        "TILES_ARCHIVE".into(),
        downloads.join("tiles.tar.gz").display().to_string(),
    );
    assert!(
        m.run(&Args::default())
            .unwrap_err()
            .contains("not a tiles-vX.Y.Z-")
    );
}

#[test]
fn another_releases_location_is_read_from_the_override() {
    let mut m = Machine::new("override");
    m.vars
        .insert("TILES_RELEASES_REPO".into(), "acme/tiles".into());
    m.run(&Args::default()).unwrap();
    let log = gh_log(&m.root);
    assert!(
        log.contains("release download v9.9.9 --repo acme/tiles "),
        "{log}"
    );
    assert!(!log.contains("saiemamer"), "{log}");
}

#[test]
fn a_tiles_off_path_or_run_from_elsewhere_is_named() {
    let mut m = Machine::new("path");
    m.vars.insert("PATH".into(), "/usr/bin".into());
    let lines = m.run(&Args::default()).unwrap();
    let dir = m.root.join("bin").display().to_string();
    assert_eq!(
        lines[1],
        format!("add {dir} to PATH, for example: echo 'export PATH=\"{dir}:$PATH\"' >> ~/.zshrc")
    );
    let env = |k: &str| m.vars.get(k).cloned();
    let lines = update(
        &Args {
            allow_older: true,
            ..Args::default()
        },
        &Context {
            env: &env,
            gh: m.gh.clone(),
            current: "10.0.0",
            target: TARGET,
            dest: m.dest(),
            running: Some(PathBuf::from("/opt/homebrew/bin/tiles")),
            work: m.root.join("work"),
            latest: &|_| Err(String::new()),
        },
    )
    .unwrap();
    assert_eq!(
        lines.last().unwrap(),
        "this tiles runs from /opt/homebrew/bin/tiles, which still holds 10.0.0"
    );
}

#[cfg(target_os = "macos")]
fn quarantine(path: &Path) {
    rustix::fs::lsetxattr(
        path,
        "com.apple.quarantine",
        b"0083;00000000;Safari;",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    assert!(quarantined(path));
}

#[cfg(target_os = "macos")]
fn quarantined(path: &Path) -> bool {
    let mut buf = [0u8; 256];
    rustix::fs::lgetxattr(path, "com.apple.quarantine", &mut buf[..]).is_ok()
}

#[cfg(not(target_os = "macos"))]
fn quarantine(_: &Path) {}

#[cfg(not(target_os = "macos"))]
fn quarantined(_: &Path) -> bool {
    false
}

#[test]
fn the_quarantine_flag_is_removed_and_a_file_without_one_is_left() {
    let dir = scratch("quarantine");
    let file = dir.join("tiles");
    script(&file, "true");
    quarantine(&file);
    super::strip_quarantine(&file).unwrap();
    assert!(!quarantined(&file));
    super::strip_quarantine(&file).unwrap();
    assert!(file.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

fn state_env(dir: &Path, extra: &[(&str, &str)]) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = extra
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    vars.insert(
        "TILES_STATE_DIR".into(),
        dir.join("state").display().to_string(),
    );
    vars
}

#[test]
fn the_check_runs_at_most_once_a_day_and_never_when_off() {
    let dir = scratch("daily");
    let vars = state_env(&dir, &[]);
    let env = |k: &str| vars.get(k).cloned();
    let asked = std::cell::RefCell::new(Vec::new());
    let fetch = |repo: &str| {
        asked.borrow_mut().push(repo.to_string());
        Ok(Version(0, 1, 5))
    };
    let t = 1_792_436_886;
    let state = refresh(&env, "0.1.4", t, &fetch).unwrap();
    assert_eq!(
        state,
        State {
            checked: Some(t),
            latest: Some(Version(0, 1, 5))
        }
    );
    assert_eq!(*asked.borrow(), ["tileshq/tiles"]);
    assert_eq!(read_state(&dir.join("state/update-check.json")), state);
    refresh(&env, "0.1.4", t + 3_600, &fetch).unwrap();
    refresh(&env, "0.1.4", t + 86_399, &fetch).unwrap();
    assert_eq!(asked.borrow().len(), 1, "once a day");
    refresh(&env, "0.1.4", t + 86_400, &fetch).unwrap();
    assert_eq!(asked.borrow().len(), 2);
    refresh(&env, "0.1.4", t, &fetch).unwrap();
    assert_eq!(asked.borrow().len(), 3, "a clock set back checks again");
    assert_eq!(
        notice("0.1.4", &state, false).as_deref(),
        Some("Tiles v0.1.5 is available; run tiles update")
    );
    assert_eq!(
        notice("0.1.4", &state, true).as_deref(),
        Some(
            "Tiles v0.1.5 is available; download tiles-v0.1.5-macos.pkg from its release and open it"
        )
    );
    assert_eq!(notice("0.1.5", &state, false), None);
    assert_eq!(notice("0.2.0", &state, true), None);

    for off in ["off", "OFF", " off ", "0", "false", "False", "no", "NO"] {
        let other = scratch("daily-off");
        let vars = state_env(&other, &[("TILES_UPDATE_CHECK", off)]);
        let env = |k: &str| vars.get(k).cloned();
        assert_eq!(refresh(&env, "0.1.4", t, &fetch), None, "{off:?}");
        assert!(!other.join("state").exists(), "off writes nothing");
        let line = doctor_line(&env, "0.1.4", None, t);
        assert!(
            line.starts_with(&format!(
                "update check: off (TILES_UPDATE_CHECK={}); ",
                off.trim()
            )),
            "{line}"
        );
    }
    for on in ["on", "1", "yes", "offline"] {
        let other = scratch("daily-on");
        let vars = state_env(&other, &[("TILES_UPDATE_CHECK", on)]);
        let env = |k: &str| vars.get(k).cloned();
        assert!(refresh(&env, "0.1.4", t, &fetch).is_some(), "{on:?}");
        let line = doctor_line(&env, "0.1.4", Some(&State::default()), t);
        assert!(
            line.starts_with(&format!(
                "update check: on (TILES_UPDATE_CHECK={on}; off, 0, false or no turns it off), "
            )),
            "{line}"
        );
        let _ = std::fs::remove_dir_all(&other);
    }
    let asked_before = asked.borrow().len();
    let bad = state_env(&dir, &[("TILES_RELEASES_REPO", "acme")]);
    let bad = |k: &str| bad.get(k).cloned();
    assert_eq!(
        refresh(&bad, "0.1.4", t + 3_600, &fetch),
        None,
        "a malformed releases location: no check, and no notice from an older one"
    );
    assert_eq!(asked.borrow().len(), asked_before);
    assert_eq!(
        refresh(&env, "0.0.0", t + 10 * 86_400, &fetch),
        None,
        "a development build never asks"
    );
    assert_eq!(
        refresh(&|_| None, "0.1.4", t, &fetch),
        None,
        "no home, no check"
    );
    assert_eq!(asked.borrow().len(), asked_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_check_counts_and_is_silent() {
    let dir = scratch("failed");
    let vars = state_env(&dir, &[]);
    let env = |k: &str| vars.get(k).cloned();
    let t = 1_792_436_886;
    let state = refresh(&env, "0.1.4", t, &|_| Err("GitHub answered 404".into())).unwrap();
    assert_eq!(
        state,
        State {
            checked: Some(t),
            latest: None
        }
    );
    assert_eq!(notice("0.1.4", &state, false), None);
    assert!(!due(&state, t + 60));
    let ok = refresh(&env, "0.1.4", t + 86_400, &|_| Ok(Version(0, 2, 0))).unwrap();
    let again = refresh(&env, "0.1.4", t + 2 * 86_400, &|_| Err(String::new())).unwrap();
    assert_eq!(
        again.latest, ok.latest,
        "a failure keeps the release seen before"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_state_lives_outside_the_cache() {
    let home = |k: &str| (k == "HOME").then(|| "/Users/u".to_string());
    assert_eq!(
        state_path(&home).unwrap(),
        PathBuf::from("/Users/u/.local/state/tiles/update-check.json")
    );
    let xdg = |k: &str| match k {
        "HOME" => Some("/Users/u".to_string()),
        "XDG_STATE_HOME" => Some("/s".to_string()),
        _ => None,
    };
    assert_eq!(
        state_path(&xdg).unwrap(),
        PathBuf::from("/s/tiles/update-check.json")
    );
    let cache = tiles_mcp::cache::default_dir(&home).unwrap();
    assert!(!state_path(&home).unwrap().starts_with(cache));
}

#[test]
fn the_metadata_tag_is_read_and_nothing_else_trusted() {
    assert_eq!(
        latest_from_metadata(br#"{"tag_name":"v0.1.5","name":"Tiles v0.1.5","body":"notes"}"#),
        Some(Version(0, 1, 5))
    );
    assert_eq!(latest_from_metadata(br#"{"message":"Not Found"}"#), None);
    assert_eq!(latest_from_metadata(b"<html>"), None);
}

#[test]
fn doctor_names_the_setting_and_the_last_run() {
    let none = |_: &str| None;
    let t = 1_792_436_886;
    let ran = State {
        checked: Some(t),
        latest: Some(Version(0, 1, 4)),
    };
    assert_eq!(
        doctor_line(&none, "0.1.4", Some(&ran), t + 3 * 3_600 + 59),
        "update check: on (TILES_UPDATE_CHECK unset; off, 0, false or no turns it off), last run 3 h ago, latest release of tileshq/tiles seen v0.1.4"
    );
    for (later, ago) in [
        (0, "0 min ago"),
        (59 * 60 + 59, "59 min ago"),
        (3_600, "1 h ago"),
        (86_399, "23 h ago"),
        (86_400, "1 d ago"),
        (9 * 86_400 + 5, "9 d ago"),
    ] {
        let line = doctor_line(&none, "0.1.4", Some(&ran), t + later);
        assert!(
            line.contains(&format!(", last run {ago}, ")),
            "{later}: {line}"
        );
    }
    assert_eq!(
        doctor_line(&none, "0.1.4", Some(&State::default()), t),
        "update check: on (TILES_UPDATE_CHECK unset; off, 0, false or no turns it off), not run yet; at most once a day it asks api.github.com for tileshq/tiles's latest release"
    );
    assert!(doctor_line(&none, "0.0.0", None, t).starts_with("update check: none for a build"));
    assert!(doctor_line(&none, "0.1.4", None, t).contains("no HOME or TILES_STATE_DIR"));
    let bad = |k: &str| (k == "TILES_RELEASES_REPO").then(|| "acme".to_string());
    for state in [None, Some(&ran), Some(&State::default())] {
        assert_eq!(
            doctor_line(&bad, "0.1.4", state, t),
            "update check: TILES_RELEASES_REPO=\"acme\" is not <owner>/<repo>, so it does not run"
        );
    }
}

#[test]
fn hostile_text_never_reaches_the_notice_or_the_doctor_line() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let dir = scratch("hostile");
    let file = dir.join("update-check.json");
    let safe = |s: &str| !s.chars().any(tiles_intent::text::is_unsafe_char);
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        let metadata = serde_json::json!({ "tag_name": e }).to_string();
        let latest = latest_from_metadata(metadata.as_bytes());
        assert!(latest.is_none() || Version::parse(e).is_some(), "{e:?}");
        std::fs::write(
            &file,
            serde_json::json!({ "checked": 1, "latest": e }).to_string(),
        )
        .unwrap();
        let state = read_state(&file);
        if let Some(n) = notice("0.0.1", &state, false) {
            assert!(safe(&n), "{n:?}");
        }
        for vars in [
            state_env(
                &dir,
                &[("TILES_RELEASES_REPO", e), ("TILES_UPDATE_CHECK", e)],
            ),
            state_env(&dir, &[("TILES_UPDATE_CHECK", e)]),
        ] {
            let env = |k: &str| vars.get(k).cloned();
            let line = doctor_line(&env, "0.1.4", Some(&state), 2);
            assert!(safe(&line), "{e:?}: {line:?}");
        }
        let refused = parse(&[e.to_string()]).err().unwrap_or_default();
        assert!(safe(&refused), "{e:?}: {refused:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tiles_the_package_installed_is_updated_by_the_next_package() {
    let update = Args::default();
    let check = Args {
        check: true,
        ..Args::default()
    };
    let dir = scratch("from-package");
    let package = dir.join("tiles");
    std::fs::write(&package, "x").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&package, &link).unwrap();
    let why = from_package(&update, false, Some(&link), &package).expect("refused");
    assert!(
        why.contains(&format!("the Tiles package in {}", package.display())),
        "{why}"
    );
    assert!(why.contains("download tiles-<tag>-macos.pkg"), "{why}");
    assert_eq!(from_package(&check, false, Some(&package), &package), None);
    assert_eq!(from_package(&update, true, Some(&package), &package), None);
    let other = dir.join("other");
    std::fs::write(&other, "x").unwrap();
    assert_eq!(from_package(&update, false, Some(&other), &package), None);
    assert_eq!(from_package(&update, false, None, &package), None);
    assert!(super::packaged(false, Some(&link), &package));
    assert!(!super::packaged(true, Some(&link), &package));
    let (old, new) = (Version::parse("0.1.4"), Version::parse("0.1.5").unwrap());
    assert_eq!(
        super::check_line(old, new, "acme/tiles", true),
        "Tiles v0.1.5 is available; download tiles-v0.1.5-macos.pkg from its release and open it",
        "never the tiles update it refuses"
    );
    assert_eq!(
        super::check_line(old, new, "acme/tiles", false),
        "Tiles v0.1.5 is available; run tiles update"
    );
    assert_eq!(
        super::PACKAGE_BIN,
        "/usr/local/bin/tiles",
        "where scripts/package-macos.sh installs it"
    );
}
