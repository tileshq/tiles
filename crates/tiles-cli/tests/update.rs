//! `tiles update` end to end (ADR-032): the binary under test, with a fake
//! `gh` first on `PATH` that serves a fake releases directory, replaces the
//! `tiles` in `TILES_BIN_DIR`; a tampered sum leaves it as it was. Never the
//! real repository or the network. Releases ship for macOS only, so these
//! run there; `src/update/tests.rs` covers the same paths on every platform.
#![cfg(target_os = "macos")]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

fn target() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    }
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A machine with `tiles` 0.1.4 in `bin/`, release v9.9.9 in `releases/`
/// and a signed-in fake `gh` in `path/`.
fn machine(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tiles-update-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["bin", "path", "releases", "stage", "home", "tmp"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    script(
        &root.join("bin/tiles"),
        "echo 'tiles 0.1.4 (intent schema 1)'",
    );
    let name = format!("tiles-v9.9.9-{}", target());
    std::fs::create_dir_all(root.join("stage").join(&name)).unwrap();
    script(
        &root.join("stage").join(&name).join("tiles"),
        "echo 'tiles 9.9.9 (intent schema 1)'",
    );
    let archive = format!("{name}.tar.gz");
    let made = Command::new("tar")
        .arg("-czf")
        .arg(root.join("releases").join(&archive))
        .arg("-C")
        .arg(root.join("stage"))
        .arg(&name)
        .status()
        .unwrap();
    assert!(made.success());
    let sum = Command::new("shasum")
        .args(["-a", "256", &archive])
        .current_dir(root.join("releases"))
        .output()
        .unwrap();
    std::fs::write(root.join("releases/SHA256SUMS"), sum.stdout).unwrap();
    script(
        &root.join("path/gh"),
        &format!(
            r#"echo "$*" >> "{root}/gh.log"
case "$1 $2" in
  "auth status") exit 0 ;;
  "release view") echo v9.9.9 ;;
  "release download")
    while [ $# -gt 0 ]; do [ "$1" = --dir ] && cp "{root}/releases/"* "$2/"; shift; done ;;
esac"#,
            root = root.display()
        ),
    );
    root
}

fn tiles(root: &Path, args: &[&str]) -> Output {
    Command::new(TILES)
        .args(args)
        .env(
            "PATH",
            format!(
                "{}:{}:/usr/bin:/bin",
                root.join("path").display(),
                root.join("bin").display()
            ),
        )
        .env("HOME", root.join("home"))
        .env("TILES_BIN_DIR", root.join("bin"))
        .env("TMPDIR", root.join("tmp"))
        .env_remove("TILES_ARCHIVE")
        .env_remove("TILES_SUMS")
        .env_remove("TILES_RELEASES_REPO")
        .output()
        .unwrap()
}

fn installed(root: &Path) -> String {
    String::from_utf8(
        Command::new(root.join("bin/tiles"))
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
}

#[test]
fn tiles_update_installs_the_latest_release_through_gh() {
    let root = machine("gh");
    let out = tiles(&root, &["update", "--check"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Tiles v9.9.9 is available; run tiles update\n"
    );
    assert_eq!(installed(&root), "tiles 0.1.4 (intent schema 1)\n");
    let out = tiles(&root, &["update"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.starts_with(&format!(
            "updated tiles from 0.0.0 to 9.9.9 at {}\n",
            root.join("bin/tiles").display()
        )),
        "{said}"
    );
    assert_eq!(installed(&root), "tiles 9.9.9 (intent schema 1)\n");
    let log = std::fs::read_to_string(root.join("gh.log")).unwrap();
    assert!(
        log.contains("release download v9.9.9 --repo tileshq/tiles "),
        "{log}"
    );
    let left: Vec<_> = std::fs::read_dir(root.join("bin")).unwrap().collect();
    assert_eq!(left.len(), 1, "only tiles in bin/");
    let left: Vec<_> = std::fs::read_dir(root.join("tmp")).unwrap().collect();
    assert!(left.is_empty(), "nothing left to unpack in");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn tiles_update_refuses_a_tampered_sum_and_keeps_tiles() {
    let root = machine("tampered");
    std::fs::write(
        root.join("releases/SHA256SUMS"),
        format!("{}  tiles-v9.9.9-{}.tar.gz\n", "0".repeat(64), target()),
    )
    .unwrap();
    let out = tiles(&root, &["update"]);
    assert!(!out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        format!(
            "tiles: tiles-v9.9.9-{}.tar.gz does not match SHA256SUMS; nothing was replaced\n",
            target()
        )
    );
    assert_eq!(installed(&root), "tiles 0.1.4 (intent schema 1)\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_gh_tiles_update_prints_the_steps_by_hand() {
    let root = machine("steps");
    std::fs::remove_file(root.join("path/gh")).unwrap();
    let out = Command::new(TILES)
        .args(["update"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("HOME", root.join("home"))
        .env("TILES_BIN_DIR", root.join("bin"))
        .env_remove("TILES_ARCHIVE")
        .env_remove("TILES_SUMS")
        .env_remove("TILES_RELEASES_REPO")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.starts_with(
            "tiles: gh is not on PATH, so tiles update cannot download the release itself."
        ),
        "{said}"
    );
    assert!(
        said.contains("\n  1. open https://github.com/tileshq/tiles/releases/latest "),
        "{said}"
    );
    assert_eq!(installed(&root), "tiles 0.1.4 (intent schema 1)\n");
    let _ = std::fs::remove_dir_all(&root);
}
