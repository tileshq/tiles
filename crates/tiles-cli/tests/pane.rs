//! `tiles pane` through a real pseudo-terminal (ADR-063): the test answers
//! the pane's probe as a terminal with pixel mouse reports does, moves the
//! pointer onto a bar it finds with the same layout code, clicks it, and
//! reads every byte the pane writes and the trace it keeps. The pane exists
//! only with debug assertions on.
#![cfg(debug_assertions)]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");
const ROWS: u16 = 60;
const COLS: u16 = 200;
const CELL: (u16, u16) = (16, 32);
const PROBE_REPLY: &[u8] = b"\x1b[?1016;2$y\x1b[6;32;16t\x1b[?62;22c";
const LEAVE: &[u8] =
    b"\x1b[?2026l\x1b[?1016l\x1b[?1006l\x1b[?1003l\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?25h\x1b[?1049l";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/pane/{name}"))
}

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// One pane at a time: each draws a whole dashboard, and several at once
/// on a loaded machine outrun the deadlines.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Pane {
    _turn: std::sync::MutexGuard<'static, ()>,
    pty: pty_process::blocking::Pty,
    child: std::process::Child,
    chunks: mpsc::Receiver<Vec<u8>>,
    seen: Vec<u8>,
    trace: PathBuf,
    dir: PathBuf,
}

impl Pane {
    fn start(name: &str, fixture: &Path, env: &[(&str, &str)]) -> Self {
        let turn = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("tiles-pane-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let trace = dir.join("trace.txt");
        let (pty, pts) = pty_process::blocking::open().unwrap();
        pty.resize(pty_process::Size::new_with_pixel(
            ROWS,
            COLS,
            COLS * CELL.0,
            ROWS * CELL.1,
        ))
        .unwrap();
        let mut cmd = pty_process::blocking::Command::new(TILES)
            .args(["pane", fixture.to_str().unwrap(), "--trace"])
            .arg(&trace)
            .env("TILES_CACHE_DIR", dir.join("cache"))
            .env("TILES_THEME", tiles_engine::Theme::default().name())
            .env("TERM", "xterm-256color")
            .env_remove("TERM_PROGRAM")
            .env_remove("TILES_SCALE")
            .env_remove("TMUX");
        for (k, v) in env {
            cmd = cmd.env(k, v);
        }
        let child = cmd.spawn(pts).unwrap();
        let reader = std::os::fd::AsFd::as_fd(&pty).try_clone_to_owned().unwrap();
        let (tx, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut from = std::fs::File::from(reader);
            let mut buf = vec![0u8; 1 << 16];
            while let Ok(n) = from.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            _turn: turn,
            pty,
            child,
            chunks,
            seen: Vec::new(),
            trace,
            dir,
        }
    }

    /// Reads until `needle` has arrived, answering the probe on the way.
    fn wait_for(&mut self, needle: &[u8]) {
        self.wait_from(0, needle);
    }

    /// Reads until `needle` has arrived at or after byte `from`, and
    /// returns where it ends.
    fn wait_from(&mut self, from: usize, needle: &[u8]) -> usize {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut answered = has(&self.seen, b"\x1b[c");
        let found = |seen: &[u8]| {
            seen.get(from..)
                .and_then(|s| s.windows(needle.len()).position(|w| w == needle))
                .map(|at| from + at + needle.len())
        };
        loop {
            if let Some(end) = found(&self.seen) {
                return end;
            }
            assert!(
                Instant::now() < deadline,
                "no {:?} in {} bytes",
                String::from_utf8_lossy(needle),
                self.seen.len()
            );
            if let Ok(chunk) = self.chunks.recv_timeout(Duration::from_millis(100)) {
                self.seen.extend_from_slice(&chunk);
            }
            if !answered && has(&self.seen, b"\x1b[c") {
                self.pty.write_all(PROBE_REPLY).unwrap();
                answered = true;
            }
        }
    }

    /// Waits for the first frame, ended by the status line and the end of
    /// its update, and returns where it ends.
    fn first_frame(&mut self) -> usize {
        let status = self.wait_from(0, b"q quits");
        self.wait_from(status, b"\x1b[?2026l")
    }

    fn send(&mut self, bytes: &[u8]) {
        self.pty.write_all(bytes).unwrap();
    }

    fn trace(&self) -> String {
        std::fs::read_to_string(&self.trace).unwrap_or_default()
    }

    /// Waits for the trace to hold `n` lines naming `what`.
    fn wait_trace(&mut self, what: &str, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while self.trace().matches(what).count() < n {
            assert!(Instant::now() < deadline, "no {what} in:\n{}", self.trace());
            if let Ok(chunk) = self.chunks.recv_timeout(Duration::from_millis(100)) {
                self.seen.extend_from_slice(&chunk);
            }
        }
    }

    fn finish(mut self) -> (std::process::ExitStatus, Vec<u8>, String) {
        let status = self.child.wait().unwrap();
        while let Ok(chunk) = self.chunks.recv_timeout(Duration::from_millis(300)) {
            self.seen.extend_from_slice(&chunk);
        }
        let trace = self.trace();
        let _ = std::fs::remove_dir_all(&self.dir);
        (status, self.seen, trace)
    }
}

/// The middle of the bar chart's first bar, in device pixels, laid out as
/// the pane lays it out for this terminal.
fn first_bar() -> (u32, u32) {
    let json = std::fs::read_to_string(fixture("dashboard.json")).unwrap();
    let painter = tiles_mcp::card::EnginePainter::new().unwrap();
    let call = tiles_mcp::card::Call::read(&json, &painter).unwrap();
    let look = tiles_mcp::card::Look {
        theme: tiles_engine::Theme::default(),
        term_cols: COLS,
        cell_px: (f32::from(CELL.0), f32::from(CELL.1)),
        ..tiles_mcp::card::Look::default()
    };
    let (laid, order) = call.lay_out(&call.intents, &painter, &look).unwrap();
    let bar = call.intents.iter().position(|i| i.form == "bar").unwrap();
    let card = laid
        .cards
        .iter()
        .filter(|c| c.align.is_some())
        .zip(&order)
        .find(|(_, i)| **i == bar)
        .unwrap()
        .0;
    let hit = &tiles_engine::hit::hit_map(&card.scene, look.scale)[0];
    let x = u32::from(card.at.0) * u32::from(CELL.0)
        + u32::try_from(i32::midpoint(hit.left, hit.right)).unwrap();
    let y = u32::from(card.at.1) * u32::from(CELL.1)
        + u32::try_from(i32::midpoint(hit.top, hit.bottom)).unwrap();
    (x, y)
}

#[test]
fn a_hover_sends_only_the_overlay_and_a_click_sends_only_the_cards_it_filters() {
    let (x, y) = first_bar();
    let mut pane = Pane::start("dashboard", &fixture("dashboard.json"), &[]);
    let first_frame = pane.first_frame();
    // SGR pixel reports are 1-based.
    pane.send(format!("\x1b[<35;{};{}M", x + 1, y + 1).as_bytes());
    let hovered = pane.wait_from(first_frame, b"\x1b[?2026l");
    let hover = pane.seen[first_frame..hovered].to_vec();
    pane.send(format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", x + 1, y + 1, x + 1, y + 1).as_bytes());
    pane.wait_trace("kind=click bytes", 1);
    pane.send(b"q");
    let (status, out, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    assert!(
        out.starts_with(b"\x1b[?1049h"),
        "the pane takes the alternate screen"
    );
    assert!(
        has(&out, b"\x1b[?1003h\x1b[?1006h\x1b[?1016h"),
        "every motion, in pixels"
    );
    assert!(out.ends_with(LEAVE), "the screen comes back last");
    // The first frame sends nine pictures: the heading and eight cards.
    assert!(trace.contains("kind=first bytes="), "{trace}");
    assert!(trace.contains("cards=9"), "{trace}");
    // The hover sends one picture, the overlay, above the cards, and no card.
    assert!(has(&hover, b"i=6999990,p=1,"), "the overlay is sent");
    assert!(!has(&hover, b"i=700"), "no card is sent again on hover");
    assert!(has(&hover, b",z=2,"));
    let line = trace
        .lines()
        .find(|l| l.contains("kind=hover bytes"))
        .unwrap();
    let bytes: usize = line
        .split("bytes=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(bytes < 64 * 1024, "{line}");
    // Clicking Germany's bar filters the four other cards that hold a
    // region column; the bar already highlights Germany, so it stays, as do
    // the three scorecards and the heading.
    let click = trace
        .lines()
        .find(|l| l.contains("kind=click bytes"))
        .unwrap();
    assert!(click.ends_with("cards=4"), "{click}");
    assert!(
        has(&out, "region = Germany".as_bytes()),
        "the status line names the selection"
    );
    for kind in [
        "rss kb=",
        "cpu ms=",
        "read bytes=",
        "begin update=1 kind=first",
    ] {
        assert!(trace.contains(kind), "{kind} in {trace}");
    }
}

#[test]
fn sigint_gives_the_screen_back() {
    let mut pane = Pane::start("sigint", &fixture("dashboard.json"), &[]);
    pane.first_frame();
    let pid = pane.child.id().to_string();
    let killed = std::process::Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .unwrap();
    assert!(killed.success());
    let (status, out, _) = pane.finish();
    assert!(status.success(), "{status:?}");
    assert!(out.ends_with(LEAVE));
}

#[test]
fn a_panic_gives_the_screen_back_before_its_message() {
    let mut pane = Pane::start(
        "panic",
        &fixture("dashboard.json"),
        &[("TILES_PANE_TEST_PANIC", "1")],
    );
    pane.wait_for(b"panicked");
    let (status, out, _) = pane.finish();
    assert!(!status.success());
    let leave = out.windows(LEAVE.len()).position(|w| w == LEAVE).unwrap();
    let panic = out.windows(8).position(|w| w == b"panicked").unwrap();
    assert!(
        leave < panic,
        "the screen comes back before the message prints"
    );
    assert_eq!(
        out.windows(LEAVE.len()).filter(|w| *w == LEAVE).count(),
        1,
        "and only once"
    );
}

#[test]
fn the_stress_grid_redraws_one_picture_per_scroll_step() {
    let mut pane = Pane::start("stress", &fixture("table-stress.json"), &[]);
    pane.first_frame();
    pane.send(b"\x1b[<65;20;20M");
    pane.wait_trace("kind=scroll bytes", 1);
    pane.send(b"\x1b[B");
    pane.wait_trace("kind=scroll bytes", 2);
    pane.send(b"\x03");
    let (status, out, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    assert!(out.ends_with(LEAVE));
    assert_eq!(trace.matches("kind=scroll bytes").count(), 2, "{trace}");
    assert!(
        trace
            .lines()
            .filter(|l| l.contains("kind=scroll bytes"))
            .all(|l| l.ends_with("cards=1"))
    );
    assert!(has(&out, "rows 3–42 of 120".as_bytes()));
}

#[test]
fn without_a_terminal_the_pane_refuses_and_writes_nothing() {
    let out = std::process::Command::new(TILES)
        .args(["pane", fixture("dashboard.json").to_str().unwrap()])
        .env(
            "TILES_CACHE_DIR",
            std::env::temp_dir().join("tiles-pane-noterm"),
        )
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(out.stdout, b"");
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs a terminal"));
}

#[test]
fn a_resize_lays_every_card_out_again_and_draws_it_from_nothing() {
    let mut pane = Pane::start("resize", &fixture("dashboard.json"), &[]);
    let first = pane.first_frame();
    pane.pty
        .resize(pty_process::Size::new_with_pixel(
            ROWS,
            COLS - 40,
            (COLS - 40) * CELL.0,
            ROWS * CELL.1,
        ))
        .unwrap();
    pane.wait_trace("kind=resize bytes", 1);
    let cleared = pane.wait_from(first, b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[2J");
    pane.wait_from(cleared, b"\x1b[?2026l");
    pane.send(b"q");
    let (status, _, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    let resize = trace
        .lines()
        .find(|l| l.contains("kind=resize bytes"))
        .unwrap();
    assert!(resize.ends_with("cards=9"), "{resize}");
}
