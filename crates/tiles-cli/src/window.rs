//! A card in a window of its own for `tiles open` (ADR-023), and the
//! companion window `tiles run` keeps on the text route (ADR-046,
//! [`crate::companion`]): on macOS the system's `osascript` runs
//! `window.js`, a native window around the card's PNG, so nothing is
//! compiled or installed for it. `tiles open`'s window outlives the command
//! and closes with Cmd-W.

use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCRIPT: &str = include_str!("window.js");
const OSASCRIPT: &str = "/usr/bin/osascript";

/// The environment variable that, set to `off`, `0`, `false` or `no`, keeps
/// `tiles open` and `tiles run` from opening a window, for scripts, tests
/// and users who want none.
pub const ENV_WINDOW: &str = "TILES_WINDOW";

/// Whether [`ENV_WINDOW`] leaves windows on: unset, or anything but `off`,
/// `0`, `false` or `no` in any case.
pub fn on(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env(ENV_WINDOW).is_none_or(|v| {
        !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false" | "no"
        )
    })
}

/// How long a window that fails takes to say so: a script error ends
/// `osascript` at once, a window keeps it running.
const SETTLE: Duration = Duration::from_millis(1500);

/// Whether `tiles open` opens a window: on macOS, in a plain shell, unless
/// [`ENV_WINDOW`] turns windows off ([`on`]). Inside `tiles run`, where `TILES_DRAW` is set, a
/// card is shown in the terminal and never in a window, so a model that
/// runs `tiles open` on the user's behalf opens nothing on their desktop
/// (ADR-027).
pub fn wanted(env: &dyn Fn(&str) -> Option<String>) -> bool {
    cfg!(target_os = "macos") && env("TILES_DRAW").is_none() && on(env)
}

/// The `osascript` call that shows `png` titled `title`, in the light or
/// dark window appearance. The path and title go as arguments, never into
/// the script.
fn command(png: &Path, title: &str, light: bool) -> Command {
    let mut cmd = Command::new(OSASCRIPT);
    cmd.args(["-l", "JavaScript", "-e", SCRIPT])
        .arg(png)
        .arg(tiles_term::sanitize::line(title, 200))
        .arg(if light { "light" } else { "dark" });
    cmd
}

/// The `osascript` call that runs the companion window over `file`, the
/// card it shows, beside the front window of the app `bundle` names (the
/// frontmost app when empty). It never activates: `window.js` opens a
/// non-activating panel and orders it front without taking focus.
pub fn follow_command(file: &Path, bundle: &str) -> Command {
    let mut cmd = Command::new(OSASCRIPT);
    cmd.args(["-l", "JavaScript", "-e", SCRIPT, "follow"])
        .arg(file)
        .arg(tiles_term::sanitize::line(bundle, 200));
    cmd
}

/// Shows `png` in a window of its own titled `title`, and returns once the
/// window is up. The window runs in its own process group, so it stays
/// when the command or the terminal ends; its stderr goes to an unlinked
/// file that outlives the command, read only when it fails to open.
///
/// # Errors
///
/// When `osascript` cannot start or ends with an error.
pub fn show(png: &Path, title: &str, light: bool) -> Result<(), String> {
    let log = png.with_file_name(format!(".window.{}.err", std::process::id()));
    let mut err = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&log)
        .map_err(|e| format!("cannot start {OSASCRIPT}: {e}"))?;
    let _ = std::fs::remove_file(&log);
    let mut cmd = command(png, title, light);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(
            err.try_clone()
                .map_err(|e| format!("cannot start {OSASCRIPT}: {e}"))?,
        )
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot start {OSASCRIPT}: {e}"))?;
    let start = Instant::now();
    while start.elapsed() < SETTLE {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut said = String::new();
                if std::io::Seek::rewind(&mut err).is_ok() {
                    let _ = std::io::Read::read_to_string(&mut err, &mut said);
                }
                return Err(format!(
                    "the window did not open ({status}): {}",
                    tiles_term::sanitize::line(said.trim(), 300)
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("cannot follow {OSASCRIPT}: {e}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ENV_WINDOW, command, follow_command, on, wanted};

    #[test]
    fn the_path_and_title_go_as_arguments_after_the_script() {
        let cmd = command(
            Path::new("/c/a1b2c3.png"),
            "Rev\u{1b}]0;x\u{7} \"$(no)\"",
            true,
        );
        assert_eq!(cmd.get_program(), "/usr/bin/osascript");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&args[..3], ["-l", "JavaScript", "-e"]);
        assert!(args[3].contains("function run(argv)"));
        assert_eq!(args[4], "/c/a1b2c3.png");
        assert!(!args[5].contains('\u{1b}') && !args[5].contains('\u{7}'));
        assert!(args[5].contains("\"$(no)\""), "kept as text: {}", args[5]);
        assert_eq!(args[6], "light");
        assert_eq!(args.len(), 7);
    }

    #[test]
    fn the_window_is_off_when_asked_inside_a_session_and_off_macos() {
        assert_eq!(wanted(&|_| None), cfg!(target_os = "macos"));
        assert!(!wanted(&|k| (k == ENV_WINDOW).then(|| "off".into())));
        for off in ["off", "0", "false", "no", " No ", "OFF"] {
            assert!(!on(&|k| (k == ENV_WINDOW).then(|| off.into())), "{off:?}");
        }
        for still in ["on", "1", "yes", ""] {
            assert!(
                on(&|k| (k == ENV_WINDOW).then(|| still.into())),
                "{still:?}"
            );
        }
        assert!(
            !wanted(&|k| (k == "TILES_DRAW").then(|| "block".into())),
            "inside tiles run the card is shown in the terminal"
        );
    }

    #[test]
    fn the_companion_window_takes_its_mode_file_and_terminal_as_arguments() {
        let cmd = follow_command(Path::new("/c/run/7/window"), "com.apple.Terminal\u{1b}[2J");
        assert_eq!(cmd.get_program(), "/usr/bin/osascript");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&args[..3], ["-l", "JavaScript", "-e"]);
        assert_eq!(
            args[4], "follow",
            "a plain word: osascript takes --follow as its own option"
        );
        assert_eq!(args[5], "/c/run/7/window");
        assert!(!args[6].contains('\u{1b}'), "{}", args[6]);
        assert_eq!(args.len(), 7);
    }

    /// Runs `window.js` with `run` replaced by one that returns `expr` as
    /// JSON, so the placement maths runs in the same JavaScript for
    /// Automation the window does.
    #[cfg(target_os = "macos")]
    fn evaluate(expr: &str) -> serde_json::Value {
        let out = std::process::Command::new(super::OSASCRIPT)
            .args(["-l", "JavaScript", "-e", super::SCRIPT, "-e"])
            .arg(format!(
                "function run() {{ return JSON.stringify({expr}); }}"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    #[cfg(target_os = "macos")]
    fn within(rect: &serde_json::Value, x: f64, top: f64, width: f64, height: f64) -> bool {
        let at = |k: &str| rect[k].as_f64().unwrap();
        x >= at("x")
            && x + width <= at("x") + at("w")
            && top - height >= at("y")
            && top <= at("y") + at("h")
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_wider_card_shifts_the_window_back_inside_the_screen() {
        // Terminal fills the left two thirds of a 1512 pt laptop screen; a
        // narrow chart opens right of it, then a 944 pt dashboard follows.
        let v = evaluate(
            "(() => {
              const laptop = { frame: { x: 0, y: 0, w: 1512, h: 982 },
                visible: { x: 0, y: 0, w: 1512, h: 945 }, scale: 2 };
              const first = place([laptop], 0, { x: 0, y: 37, w: 1008, h: 900 }, { w: 640, h: 400 });
              const titleBar = 28;
              return { first: fitWindow(first, { w: 640, h: 400 }, laptop, titleBar),
                at: fitWindow(first, { w: 1888, h: 1200 }, laptop, titleBar),
                screen: laptop.visible, titleBar: titleBar };
            })()",
        );
        let n = |p: &serde_json::Value, k: &str| p[k].as_f64().unwrap();
        let bar = n(&v, "titleBar");
        let first = &v["first"];
        assert_eq!(n(first, "x"), 1020.0, "right of the terminal: {v}");
        assert_eq!((n(first, "w"), n(first, "h")), (320.0, 200.0));
        assert!(
            within(
                &v["screen"],
                n(first, "x"),
                n(first, "top"),
                320.0,
                200.0 + bar
            ),
            "{v}"
        );
        let at = &v["at"];
        assert_eq!((n(at, "w"), n(at, "h")), (944.0, 600.0), "{v}");
        assert_eq!(n(at, "x"), 1512.0 - 944.0, "flush with the right edge: {v}");
        assert_eq!(n(at, "top"), n(first, "top"), "the top stays: {v}");
        assert!(
            within(&v["screen"], n(at, "x"), n(at, "top"), 944.0, 600.0 + bar),
            "{v}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn the_card_is_sized_for_the_screen_that_gets_the_window() {
        // A 2x laptop is the main screen; Terminal sits on a 1x external
        // monitor to its right, so the card is twice as wide there.
        let v = evaluate(
            "(() => {
              const laptop = { frame: { x: 0, y: 0, w: 1512, h: 982 },
                visible: { x: 0, y: 0, w: 1512, h: 945 }, scale: 2 };
              const external = { frame: { x: 1512, y: 0, w: 1920, h: 1080 },
                visible: { x: 1512, y: 0, w: 1920, h: 1055 }, scale: 1 };
              return { at: place([laptop, external], 0, { x: 1512, y: -73, w: 1280, h: 1000 },
                { w: 800, h: 500 }), screen: external.visible };
            })()",
        );
        let at = &v["at"];
        let n = |k: &str| at[k].as_f64().unwrap();
        assert_eq!(at["screen"], 1, "{v}");
        assert_eq!((n("w"), n("h")), (800.0, 500.0), "points at 1x: {v}");
        assert!(
            within(&v["screen"], n("x"), n("top"), n("w"), n("h")),
            "{v}"
        );
        assert_eq!(n("x"), 1512.0 + 1920.0 - 800.0, "no room beside it: {v}");
    }
}
