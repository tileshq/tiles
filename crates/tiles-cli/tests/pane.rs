//! `tiles pane` through a real pseudo-terminal (ADR-063, ADR-065): the test
//! answers the pane's probe as a terminal with pixel mouse reports does,
//! moves the pointer onto a mark or a legend entry it finds with the same
//! layout code, clicks it or presses a key, and reads every byte the pane
//! writes and the trace it keeps. The panic and the stress grid exist only
//! with debug assertions on.

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
        let cache = env
            .iter()
            .find(|(k, _)| *k == "TILES_CACHE_DIR")
            .map_or_else(|| dir.join("cache"), |(_, v)| PathBuf::from(v));
        let mut cmd = pty_process::blocking::Command::new(TILES)
            .args(["pane", fixture.to_str().unwrap(), "--trace"])
            .arg(&trace)
            .env("TILES_CACHE_DIR", cache)
            .env("TILES_THEME", tiles_engine::Theme::default().name())
            .env("TERM", "xterm-256color")
            .env_remove("TERM_PROGRAM")
            .env_remove("TILES_SCALE")
            .env_remove("TMUX");
        for (k, v) in env.iter().filter(|(k, _)| *k != "TILES_CACHE_DIR") {
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
                "no {:?} in {} bytes, ending {:?}",
                String::from_utf8_lossy(needle),
                self.seen.len(),
                String::from_utf8_lossy(&self.seen[self.seen.len().saturating_sub(300)..])
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
        let status = self.wait_from(0, b" quit");
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

/// The dashboard fixture laid out as the pane lays it out for this
/// terminal.
struct Dashboard {
    painter: tiles_mcp::card::EnginePainter,
    call: tiles_mcp::card::Call,
    look: tiles_mcp::card::Look,
    laid: tiles_mcp::card::Laid,
    order: Vec<usize>,
}

impl Dashboard {
    fn new() -> Self {
        let json = std::fs::read_to_string(fixture("dashboard.json")).unwrap();
        let painter = tiles_mcp::card::EnginePainter::new().unwrap();
        let refuse = |_: tiles_mcp::card::Query<'_>, _: Instant| -> Result<_, String> {
            Err("the fixture holds its rows".into())
        };
        let call = tiles_mcp::card::Call::read(&json, &painter, &refuse).unwrap();
        let look = tiles_mcp::card::Look {
            theme: tiles_engine::Theme::default(),
            term_cols: COLS,
            cell_px: (f32::from(CELL.0), f32::from(CELL.1)),
            ..tiles_mcp::card::Look::default()
        };
        let (laid, order) = call.lay_out_pane(&call.intents, &painter, &look).unwrap();
        Self {
            painter,
            call,
            look,
            laid,
            order,
        }
    }

    /// The intent of the card drawn as `form`, and the card.
    fn card(&self, form: &str) -> (&tiles_intent::Intent, &tiles_mcp::card::LaidCard) {
        let k = self
            .call
            .intents
            .iter()
            .position(|i| i.form == form)
            .unwrap();
        let card = self
            .laid
            .cards
            .iter()
            .filter(|c| c.align.is_some())
            .zip(&self.order)
            .find(|(_, i)| **i == k)
            .unwrap()
            .0;
        (&self.call.intents[k], card)
    }
}

/// Device pixel `x`, `y` of `card` on the pane.
fn on_pane(card: &tiles_mcp::card::LaidCard, x: i32, y: i32) -> (u32, u32) {
    (
        u32::from(card.at.0) * u32::from(CELL.0) + u32::try_from(x).unwrap(),
        u32::from(card.at.1) * u32::from(CELL.1) + u32::try_from(y).unwrap(),
    )
}

/// The middle of the bar chart's first bar, in device pixels, laid out as
/// the pane lays it out for this terminal.
fn first_bar() -> (u32, u32) {
    let board = Dashboard::new();
    let (_, card) = board.card("bar");
    let hit = &tiles_engine::hit::hit_map(&card.scene, board.look.scale)[0];
    on_pane(
        card,
        i32::midpoint(hit.left, hit.right),
        i32::midpoint(hit.top, hit.bottom),
    )
}

/// The middle of the first mark of the card drawn as `form`, in device
/// pixels on the pane.
fn first_mark(board: &Dashboard, form: &str) -> (u32, u32) {
    let (_, card) = board.card(form);
    let hit = &tiles_engine::hit::hit_map(&card.scene, board.look.scale)[0];
    on_pane(
        card,
        i32::midpoint(hit.left, hit.right),
        i32::midpoint(hit.top, hit.bottom),
    )
}

/// The update lines of kind `kind` in a trace.
fn updates<'a>(trace: &'a str, kind: &str) -> Vec<&'a str> {
    let needle = format!("kind={kind} bytes=");
    trace.lines().filter(|l| l.contains(&needle)).collect()
}

/// A pointer an SGR pixel report places at `x`, `y`; the report is 1-based.
fn report(button: u8, x: u32, y: u32, end: char) -> Vec<u8> {
    format!("\x1b[<{button};{};{}{end}", x + 1, y + 1).into_bytes()
}

#[test]
fn a_hover_sends_only_the_overlay_and_a_click_sends_only_the_cards_it_focuses() {
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
    // The first frame lays the page colour under everything, below the text.
    assert!(
        has(&out, b"i=6999980,p=1,s=1,v=1,c=200,r=60,z=-1073741825"),
        "the page"
    );
    // Clicking Germany's bar focuses Germany: the lollipop, which named
    // Spain, now names Germany; the bar, the columns and the lines, which
    // already named it, take the focus ring and dim their other marks; and
    // the heading takes the chip. The stacked bars colour by method and the
    // scorecards hold no region: they stay.
    let click = trace
        .lines()
        .find(|l| l.contains("kind=click bytes"))
        .unwrap();
    assert!(click.ends_with("cards=5"), "{click}");
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
fn a_click_keeps_the_tooltip_on_the_mark_still_under_the_pointer() {
    let (x, y) = first_bar();
    let mut pane = Pane::start("click-tooltip", &fixture("dashboard.json"), &[]);
    let first_frame = pane.first_frame();
    pane.send(&report(35, x, y, 'M'));
    let hovered = pane.wait_from(first_frame, b"\x1b[?2026l");
    // Click without moving: Germany is focused and the bar under the
    // pointer keeps its tooltip, whose hint now says a click clears it.
    pane.send(&[report(0, x, y, 'M'), report(0, x, y, 'm')].concat());
    let clicked = pane.wait_from(hovered, b"\x1b[?2026l");
    let click = pane.seen[hovered..clicked].to_vec();
    // Click again: the focus clears and the tooltip stays.
    pane.send(&[report(0, x, y, 'M'), report(0, x, y, 'm')].concat());
    let cleared = pane.wait_from(clicked, b"\x1b[?2026l");
    let clear = pane.seen[clicked..cleared].to_vec();
    pane.send(b"q");
    let (status, _, trace) = pane.finish();
    assert!(status.success(), "{status:?}");

    let clicks: Vec<&str> = trace
        .lines()
        .filter(|l| l.contains("kind=click bytes"))
        .collect();
    assert_eq!(clicks.len(), 2, "{trace}");
    // Focusing and clearing change the same cards back and forth.
    assert!(clicks[0].ends_with("cards=5"), "{}", clicks[0]);
    assert!(clicks[1].ends_with("cards=5"), "{}", clicks[1]);

    // The first click places the bar's tooltip again, as the other overlay
    // id, above the cards, and deletes the one the hover placed.
    assert!(
        has(&click, b"i=6999991,p=1,"),
        "the tooltip is placed again"
    );
    assert!(has(&click, b",z=2,"));
    assert!(has(&click, b"\x1b_Ga=d,d=I,q=2,i=6999990\x1b\\"));
    let placed = click
        .windows(b"i=6999991,p=1,".len())
        .position(|w| w == b"i=6999991,p=1,")
        .unwrap();
    let last_card = click
        .windows(b"i=700".len())
        .rposition(|w| w == b"i=700")
        .unwrap();
    assert!(last_card < placed, "the tooltip goes above the new cards");
    // The second places it back under the first id.
    assert!(has(&clear, b"i=6999990,p=1,"));
    assert!(has(&clear, b"\x1b_Ga=d,d=I,q=2,i=6999991\x1b\\"));
    assert_eq!(
        trace.matches("kind=click card=overlay").count(),
        2,
        "each click draws the tooltip again: {trace}"
    );
}

#[test]
fn a_click_on_the_focused_value_on_another_card_clears_the_focus() {
    let board = Dashboard::new();
    let (column, card) = board.card("column");
    let hit = &tiles_engine::hit::hit_map(&card.scene, board.look.scale)[0];
    let x = column.column("x").unwrap();
    assert_eq!(column.table.rows[hit.row.unwrap()][x].label(), "Germany");
    let (cx, cy) = first_mark(&board, "column");
    let (bx, by) = first_bar();
    let mut pane = Pane::start("focus-elsewhere", &fixture("dashboard.json"), &[]);
    pane.first_frame();
    // Focus Germany on the bar chart, then click Germany on the columns.
    pane.send(&[report(0, bx, by, 'M'), report(0, bx, by, 'm')].concat());
    pane.wait_trace("kind=click bytes", 1);
    pane.send(&[report(0, cx, cy, 'M'), report(0, cx, cy, 'm')].concat());
    pane.wait_trace("kind=click bytes", 2);
    pane.send(b"q");
    let (status, _, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    let clicks = updates(&trace, "click");
    assert_eq!(clicks.len(), 2, "{trace}");
    // The second click clears the focus: the four cards and the heading go
    // back, as a second click on the bar itself does.
    assert!(clicks[0].ends_with("cards=5"), "{}", clicks[0]);
    assert!(clicks[1].ends_with("cards=5"), "{}", clicks[1]);
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

#[cfg(debug_assertions)]
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

#[cfg(debug_assertions)]
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

// ADR-065: a click on a legend entry hides its series on that card only,
// and `r` shows it again.
#[test]
fn a_legend_click_hides_its_series_and_r_brings_it_back() {
    let board = Dashboard::new();
    let (stacked, card) = board.card("stacked-bar");
    let legend = board.painter.engine().legend(&card.scene, board.look.scale);
    let entry = legend
        .iter()
        .find(|l| l.label == "Card")
        .unwrap_or_else(|| panic!("a legend entry for Card in {legend:?}"));
    let method = stacked.table.index("method").unwrap();
    assert!(
        stacked
            .table
            .rows
            .iter()
            .any(|r| r[method].label() == "Card")
    );
    let (x, y) = on_pane(
        card,
        i32::midpoint(entry.left, entry.right),
        i32::midpoint(entry.top, entry.bottom),
    );
    let mut pane = Pane::start("legend", &fixture("dashboard.json"), &[]);
    pane.first_frame();
    pane.send(&[report(0, x, y, 'M'), report(0, x, y, 'm')].concat());
    pane.wait_trace("kind=click bytes", 1);
    pane.send(b"r");
    pane.wait_trace("kind=reset bytes", 1);
    pane.send(b"q");
    let (status, out, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    let clicks = updates(&trace, "click");
    assert!(
        clicks[0].ends_with("cards=1"),
        "only the stacked bars: {trace}"
    );
    assert!(has(&out, b"1 hidden"), "the status line counts it");
    assert!(updates(&trace, "reset")[0].ends_with("cards=1"), "{trace}");
}

// ADR-065: `s` steps the order of the card under the pointer, sending that
// card alone; a card that ranks itself says so and sends nothing.
#[test]
fn s_sorts_the_card_under_the_pointer_and_a_ranked_card_says_it_keeps_its_order() {
    let board = Dashboard::new();
    let column = first_mark(&board, "column");
    let bar = first_mark(&board, "bar");
    let mut pane = Pane::start("sort", &fixture("dashboard.json"), &[]);
    let first = pane.first_frame();
    pane.send(&report(35, column.0, column.1, 'M'));
    let hovered = pane.wait_from(first, b"\x1b[?2026l");
    pane.send(b"s");
    pane.wait_trace("kind=sort bytes", 1);
    // The fixture's columns stand largest first already, so the first step
    // is smallest first.
    let sorted = pane.wait_from(hovered, b"smallest first");
    pane.send(&report(35, bar.0, bar.1, 'M'));
    pane.wait_from(sorted, b"\x1b[?2026l");
    pane.send(b"s");
    pane.wait_trace("kind=sort bytes", 2);
    pane.send(b"q");
    let (status, out, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    let sorts = updates(&trace, "sort");
    assert!(sorts[0].ends_with("cards=1"), "{trace}");
    assert!(sorts[1].ends_with("cards=0"), "{trace}");
    assert!(has(&out, b"a bar keeps its own order"));
}

// ADR-065: `d` on a card whose rows came with the call names what a drill
// needs and asks no source.
#[test]
fn d_on_a_card_with_rows_of_its_own_says_what_a_drill_needs() {
    let board = Dashboard::new();
    let (x, y) = first_mark(&board, "column");
    let mut pane = Pane::start("drill", &fixture("dashboard.json"), &[]);
    let first = pane.first_frame();
    pane.send(&report(35, x, y, 'M'));
    pane.wait_from(first, b"\x1b[?2026l");
    pane.send(b"d");
    pane.wait_trace("kind=drill bytes", 1);
    pane.send(b"q");
    let (status, out, trace) = pane.finish();
    assert!(status.success(), "{status:?}");
    assert!(updates(&trace, "drill")[0].ends_with("cards=0"), "{trace}");
    assert!(has(&out, b"Omni date field"));
}

// ADR-065: a card id opens the call the chart tool drew it from.
#[test]
fn a_card_id_opens_the_call_its_card_was_drawn_from() {
    let dir = std::env::temp_dir().join(format!("tiles-pane-saved-card-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cache = dir.join("cache");
    let call = dir.join("call.json");
    std::fs::write(
        &call,
        r#"{"v":1,"form":"column","title":"Germany places the most orders",
        "claim":{"kind":"rank","tone":"good"},"fields":{"x":"region","y":"orders"},
        "highlight":["Germany"],"data":{"rows":[{"region":"Germany","orders":5},
        {"region":"France","orders":3},{"region":"Spain","orders":2}]}}"#,
    )
    .unwrap();
    let drawn = std::process::Command::new(TILES)
        .args(["card", call.to_str().unwrap()])
        .env("TILES_CACHE_DIR", &cache)
        .env_remove("TILES_OMNI_URL")
        .env_remove("TILES_OMNI_API_KEY")
        .output()
        .unwrap();
    assert!(drawn.status.success(), "{drawn:?}");
    let reply: serde_json::Value = serde_json::from_slice(&drawn.stdout).unwrap();
    let id = reply["card"].as_str().unwrap().to_string();
    let cache = cache.to_str().unwrap().to_string();
    let mut pane = Pane::start(
        "card-id",
        Path::new(&id),
        &[("TILES_CACHE_DIR", cache.as_str())],
    );
    pane.first_frame();
    pane.send(b"q");
    let (status, out, trace) = pane.finish();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.success(), "{status:?}");
    assert!(has(&out, format!("card {id}").as_bytes()));
    assert!(trace.contains("kind=first bytes="), "{trace}");
    assert!(trace.contains("cards=1"), "{trace}");
}

#[test]
fn a_missing_card_id_is_refused_in_one_line() {
    let dir = std::env::temp_dir().join(format!("tiles-pane-no-card-{}", std::process::id()));
    let out = std::process::Command::new(TILES)
        .args(["pane", "abc123"])
        .env("TILES_CACHE_DIR", &dir)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(out.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "tiles pane: no card abc123 in the cache, or it was drawn before Tiles kept calls\n"
    );
}

// Definition of done 20: a refusal before the pane takes the screen goes
// to stderr in one clean line, whatever hostile text it echoes.
#[test]
fn a_refusal_that_echoes_hostile_text_is_one_clean_line() {
    let dir = std::env::temp_dir().join(format!("tiles-pane-hostile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let call = dir.join("call.json");
    // A NUL can be neither an argument nor a path.
    for e in corpus["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .filter(|e| !e.contains('\0'))
    {
        std::fs::write(
            &call,
            serde_json::json!({
                "v": 1, "form": "column", "title": "Orders by region",
                "fields": {"x": e, "y": "orders"},
                "data": {"rows": [{"region": "Germany", "orders": 5}]}
            })
            .to_string(),
        )
        .unwrap();
        let flag = format!("-{e}");
        for arg in [e, flag.as_str(), call.to_str().unwrap()] {
            let out = std::process::Command::new(TILES)
                .args(["pane", arg])
                .env("TILES_CACHE_DIR", dir.join("cache"))
                .env_remove("TILES_OMNI_URL")
                .env_remove("TILES_OMNI_API_KEY")
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{e:?} {arg:?}: {err:?}");
            assert_eq!(out.stdout, b"", "{e:?} {arg:?}");
            assert!(err.starts_with("tiles pane: "), "{e:?} {arg:?}: {err:?}");
            assert_eq!(err.matches('\n').count(), 1, "{e:?} {arg:?}: {err:?}");
            assert!(err.ends_with('\n'), "{e:?} {arg:?}: {err:?}");
            assert!(
                !err.trim_end_matches('\n')
                    .chars()
                    .any(tiles_intent::forbidden),
                "{e:?} {arg:?}: {err:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
