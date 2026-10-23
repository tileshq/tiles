//! The terminal wrapper behind `tiles run` where no harness hook draws the
//! card (ADR-004): the harness runs in a pseudo-terminal, its output passes
//! through [`paint::Painter`], and the user's keys pass straight through.
//!
//! It is a compatibility layer: it never edits what the harness wrote except
//! to draw over placeholder blocks, and a harness that gains a hook that draws
//! in a terminal moves off it there.

pub mod escape;
pub mod paint;
pub mod pictures;

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tiles_term::detect::{QUERIES, Replies, Route};

/// How long the startup query waits for the terminal's answers.
pub const QUERY_WAIT: Duration = Duration::from_millis(600);

/// What the wrapper runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The harness program.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// Variables added to its environment.
    pub env: Vec<(String, String)>,
}

enum Event {
    Output(Vec<u8>),
    Resize(u16, u16),
    Tick,
    Done,
}

/// What the wrapper knows of the terminal beyond the route.
#[derive(Debug, Clone, Copy, Default)]
pub struct Motion {
    /// Play entrances: in kitty, place cards early too.
    pub on: bool,
    /// Pre-cut: send each entrance frame as it is placed, not ahead
    /// (`TILES_PRECUT_SEND=tick`).
    pub each_tick: bool,
    /// The terminal's default background, from `OSC 11`.
    pub background: Option<(u8, u8, u8)>,
    /// The run's token: only cards carrying it are drawn or sent ahead.
    pub session: u64,
    /// The harness's side panel ([`crate::harness::Harness::side_panel`]):
    /// no card draws over it.
    pub panel: Option<(u16, u16)>,
}

/// Writes the conversation's width in a terminal `cols` wide, and the
/// terminal's `rows`, to the file the MCP server of session `session` reads
/// on every call ([`conversation_file`]), so a card made after a resize fits.
fn note_size(
    cache_dir: &std::path::Path,
    session: u64,
    panel: Option<(u16, u16)>,
    (rows, cols): (u16, u16),
) {
    let width = paint::conversation_cols(panel, cols);
    let _ = std::fs::write(
        conversation_file(cache_dir, session),
        format!("{width} {rows}"),
    );
}

/// The file holding session `session`'s conversation width in columns and
/// the terminal's height in rows.
#[must_use]
pub fn conversation_file(cache_dir: &std::path::Path, session: u64) -> std::path::PathBuf {
    cache_dir.join(format!("cols-{session:016x}"))
}

/// Puts the user's terminal back as it was, however the wrapper ends.
struct RawGuard(Option<rustix::termios::Termios>);

impl RawGuard {
    fn enter() -> Self {
        let stdin = rustix::stdio::stdin();
        let Ok(saved) = rustix::termios::tcgetattr(stdin) else {
            return Self(None);
        };
        let mut raw = saved.clone();
        raw.make_raw();
        let _ = rustix::termios::tcsetattr(stdin, rustix::termios::OptionalActions::Now, &raw);
        Self(Some(saved))
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if let Some(saved) = &self.0 {
            let _ = rustix::termios::tcsetattr(
                rustix::stdio::stdin(),
                rustix::termios::OptionalActions::Now,
                saved,
            );
        }
    }
}

fn size() -> (u16, u16) {
    rustix::termios::tcgetwinsize(rustix::stdio::stdout())
        .ok()
        .filter(|w| w.ws_row > 0 && w.ws_col > 0)
        .map_or((24, 80), |w| (w.ws_row, w.ws_col))
}

/// The pixel size of one cell, from the window size when the terminal
/// reports it.
#[must_use]
pub fn winsize_cell_px() -> Option<(u16, u16)> {
    cell_px_of(rustix::stdio::stdout())
}

/// The pixel size of one cell of the terminal `fd`, from its window size.
#[must_use]
pub fn cell_px_of(fd: impl AsFd) -> Option<(u16, u16)> {
    let w = rustix::termios::tcgetwinsize(fd).ok()?;
    (w.ws_xpixel > 0 && w.ws_ypixel > 0 && w.ws_col > 0 && w.ws_row > 0)
        .then(|| (w.ws_xpixel / w.ws_col, w.ws_ypixel / w.ws_row))
}

/// Starts reading the user's keys into a channel. Only the wrapper reads
/// them this way, once the startup query is over, so a harness launched
/// directly is the only reader of its terminal.
fn read_keys() -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

/// The user's terminal while Tiles owns it, in raw mode.
pub struct Session {
    raw: RawGuard,
}

impl Session {
    /// Takes the terminal, when standard input and output are one.
    #[must_use]
    pub fn start() -> Option<Self> {
        let tty = |fd| rustix::termios::isatty(fd);
        if !tty(rustix::stdio::stdin()) || !tty(rustix::stdio::stdout()) {
            return None;
        }
        Some(Self {
            raw: RawGuard::enter(),
        })
    }

    /// Asks the terminal what it is (kitty graphics, XTVERSION, cell size,
    /// background, device attributes). Keys sent after the replies stay
    /// unread for the harness.
    #[allow(clippy::unused_self)] // The session proves raw mode is on, so replies arrive unechoed.
    pub fn query(&mut self) -> Replies {
        let mut out = io::stdout();
        if out
            .write_all(QUERIES.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            return Replies::default();
        }
        collect_replies(rustix::stdio::stdin())
    }
}

/// The controlling terminal, `/dev/tty`, in raw mode until dropped, for
/// `tiles check` and `tiles doctor` when their output goes to a pipe.
pub struct Tty {
    file: std::fs::File,
    saved: Option<rustix::termios::Termios>,
}

impl Tty {
    /// The controlling terminal.
    pub const PATH: &str = "/dev/tty";

    /// Opens `path` and puts it in raw mode.
    ///
    /// # Errors
    ///
    /// When it cannot be opened or is not a terminal.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let shown = || tiles_term::sanitize::line(&path.display().to_string(), 200);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("{} cannot be opened: {e}", shown()))?;
        if !rustix::termios::isatty(&file) {
            return Err(format!("{} is not a terminal", shown()));
        }
        let saved = rustix::termios::tcgetattr(&file).ok();
        if let Some(saved) = &saved {
            let mut raw = saved.clone();
            raw.make_raw();
            let _ = rustix::termios::tcsetattr(&file, rustix::termios::OptionalActions::Now, &raw);
        }
        Ok(Self { file, saved })
    }

    /// Asks the terminal what it is, as [`Session::query`] does.
    pub fn query(&mut self) -> Replies {
        if self
            .file
            .write_all(QUERIES.as_bytes())
            .and_then(|()| self.file.flush())
            .is_err()
        {
            return Replies::default();
        }
        collect_replies(self.file.as_fd())
    }

    /// The terminal itself, for its window size.
    #[must_use]
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            let _ = rustix::termios::tcsetattr(
                &self.file,
                rustix::termios::OptionalActions::Now,
                saved,
            );
        }
    }
}

/// The terminal `tiles check` and `tiles doctor` ask: standard input and
/// output when both are one, else `/dev/tty` when one of them is, so a
/// report piped into a file still describes the terminal it ran in, and
/// none when neither is. `tiles run` keeps [`Session`], since the wrapper
/// owns standard input and output.
pub enum Probe {
    /// Standard input and output.
    Stdio(Session),
    /// The controlling terminal.
    Tty(Tty),
}

impl Probe {
    /// The terminal to ask.
    ///
    /// # Errors
    ///
    /// Why neither standard input and output nor `/dev/tty` can be asked.
    pub fn open() -> Result<Self, String> {
        Self::open_at(std::path::Path::new(Tty::PATH))
    }

    /// [`Probe::open`] with `tty` as the controlling terminal.
    ///
    /// # Errors
    ///
    /// As [`Probe::open`].
    pub fn open_at(tty: &std::path::Path) -> Result<Self, String> {
        if let Some(s) = Session::start() {
            return Ok(Self::Stdio(s));
        }
        let terminal = |fd| rustix::termios::isatty(fd);
        let stdio = (
            terminal(rustix::stdio::stdin()),
            terminal(rustix::stdio::stdout()),
        );
        Self::beside(stdio, tty)
    }

    /// `tty` when one of standard input and output (`stdio`) is a
    /// terminal: a report piped from a shell (`tiles check | tee`) still
    /// describes the terminal it ran in, while a run with neither, such as
    /// an agent's, asks no terminal and writes nothing to one.
    fn beside(stdio: (bool, bool), tty: &std::path::Path) -> Result<Self, String> {
        if stdio == (false, false) {
            return Err("stdin and stdout are not terminals".to_string());
        }
        Tty::open(tty)
            .map(Self::Tty)
            .map_err(|e| format!("stdin or stdout is not a terminal and {e}"))
    }

    /// Asks the terminal what it is.
    pub fn query(&mut self) -> Replies {
        match self {
            Self::Stdio(s) => s.query(),
            Self::Tty(t) => t.query(),
        }
    }

    /// The terminal's rows and columns, when it reports them.
    #[must_use]
    pub fn rows_cols(&self) -> Option<(u16, u16)> {
        rustix::termios::tcgetwinsize(self.fd())
            .ok()
            .filter(|w| w.ws_row > 0 && w.ws_col > 0)
            .map(|w| (w.ws_row, w.ws_col))
    }

    /// One cell from the terminal's window size, when it reports one.
    #[must_use]
    pub fn cell_px(&self) -> Option<(u16, u16)> {
        cell_px_of(self.fd())
    }

    fn fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Stdio(_) => rustix::stdio::stdout(),
            Self::Tty(t) => t.fd(),
        }
    }
}

/// Reads one byte at a time and stops at the device attributes reply, so
/// keys that arrive in the same write as it are left for the next reader.
fn collect_replies(input: BorrowedFd<'_>) -> Replies {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    let start = Instant::now();
    let mut seen = Vec::new();
    let mut byte = [0u8; 1];
    while let Some(left) = QUERY_WAIT.checked_sub(start.elapsed()) {
        let Ok(left) = Timespec::try_from(left) else {
            break;
        };
        match poll(
            &mut [PollFd::from_borrowed_fd(input, PollFlags::IN)],
            Some(&left),
        ) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        match rustix::io::read(input, &mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                seen.push(byte[0]);
                if Replies::parse(&seen).complete {
                    break;
                }
            }
        }
    }
    Replies::parse(&seen)
}

/// Sends a tick every `period` until the loop stops listening.
fn tick_every(period: Duration, tx: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(period);
            if tx.send(Event::Tick).is_err() {
                break;
            }
        }
    });
}

/// Follows the terminal's size every 200 ms from `start`, passing a change
/// to the harness's pseudo-terminal `pty` and to the paint loop.
fn watch_size(pty: std::os::fd::OwnedFd, start: (u16, u16), tx: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        let mut last = start;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let now = size();
            if now != last {
                last = now;
                let ws = rustix::termios::Winsize {
                    ws_row: now.0,
                    ws_col: now.1,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                let _ = rustix::termios::tcsetwinsize(&pty, ws);
                if tx.send(Event::Resize(now.0, now.1)).is_err() {
                    break;
                }
            }
        }
    });
}

/// Appends the painter's event lines to the `TILES_TRACE` file, and a line
/// for a write to the terminal that took 20 ms or more, from `at` to `now`.
fn write_trace<P: paint::Pictures>(
    file: &mut std::fs::File,
    painter: &mut paint::Painter<P>,
    at: Duration,
    now: Duration,
    bytes: usize,
) {
    let mut lines = painter.take_trace();
    let took = now.saturating_sub(at);
    if took >= Duration::from_millis(20) {
        lines.push(format!(
            "{:9.1} wrote {bytes} bytes to the terminal in {:.1} ms",
            at.as_secs_f64() * 1000.0,
            took.as_secs_f64() * 1000.0
        ));
    }
    for line in lines {
        let _ = writeln!(file, "{line}");
    }
}

/// The line that starts a session in the `TILES_TRACE` file, which `tiles
/// check` finds the last session and its cards by.
#[must_use]
pub fn trace_header(session: u64, route: Route, (rows, cols): (u16, u16)) -> String {
    format!(
        "{TRACE_SESSION}{session:016x} on the {} route, {rows} rows by {cols} columns",
        route.name()
    )
}

/// What [`trace_header`] starts with, before the session's token.
pub const TRACE_SESSION: &str = "tiles session ";

/// The `TILES_TRACE` file opened for appending, with this session's
/// [`trace_header`] written; `None` when unset or it cannot be opened.
fn open_trace(session: u64, route: Route, size: (u16, u16)) -> Option<std::fs::File> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("TILES_TRACE")?)
        .ok()?;
    let _ = writeln!(file, "{}", trace_header(session, route, size));
    Some(file)
}

/// Runs the harness under the wrapper and returns its exit code.
///
/// # Errors
///
/// When the pseudo-terminal cannot be opened or the harness cannot start.
pub fn run(
    session: Session,
    launch: &Launch,
    route: Route,
    motion: Motion,
    cache_dir: &std::path::Path,
) -> io::Result<i32> {
    let Session { raw } = session;
    let (rows, cols) = size();
    let (pty, pts) = pty_process::blocking::open().map_err(io::Error::other)?;
    pty.resize(pty_process::Size::new(rows, cols))
        .map_err(io::Error::other)?;
    // Before the harness starts, so every card it makes counts as this run's.
    let pictures = pictures::CachePictures::new(cache_dir, motion.session);
    let mut child = pty_process::blocking::Command::new(&launch.program)
        .args(&launch.args)
        .envs(launch.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn(pts)
        .map_err(|e| {
            io::Error::other(format!(
                "cannot start {}: {e}",
                tiles_term::sanitize::line(&launch.program, 80)
            ))
        })?;
    let master = pty.as_fd().try_clone_to_owned()?;
    let mut to_child = std::fs::File::from(master);
    let keys = read_keys();
    std::thread::spawn(move || {
        for bytes in keys {
            if to_child.write_all(&bytes).is_err() {
                break;
            }
        }
    });
    let (tx, rx) = mpsc::channel();
    let reader = pty.as_fd().try_clone_to_owned()?;
    let out_tx = tx.clone();
    std::thread::spawn(move || {
        let mut from_child = std::fs::File::from(reader);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match from_child.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = out_tx.send(Event::Done);
                    break;
                }
                Ok(n) => {
                    if out_tx.send(Event::Output(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });
    tick_every(paint::TICK, tx.clone());
    watch_size(pty.as_fd().try_clone_to_owned()?, (rows, cols), tx);
    let mut painter = paint::Painter::new(rows, cols, route, pictures)
        .with_panel(motion.panel)
        .with_session(motion.session);
    note_size(cache_dir, motion.session, motion.panel, (rows, cols));
    if motion.on {
        painter = painter.with_motion(motion.background);
    }
    if motion.each_tick {
        painter = painter.with_frames_each_tick();
    }
    let mut trace = open_trace(motion.session, route, (rows, cols));
    if trace.is_some() {
        painter = painter.with_trace();
    }
    let started = Instant::now();
    let mut stdout = io::stdout().lock();
    for event in rx {
        painter.set_now(started.elapsed());
        let bytes = match event {
            Event::Output(chunk) => painter.feed(&chunk),
            Event::Resize(r, c) => {
                note_size(cache_dir, motion.session, motion.panel, (r, c));
                painter.resize(r, c)
            }
            Event::Tick => painter.tick(started.elapsed()),
            Event::Done => break,
        };
        let at = started.elapsed();
        if stdout
            .write_all(&bytes)
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
        if let Some(file) = &mut trace {
            write_trace(file, &mut painter, at, started.elapsed(), bytes.len());
        }
    }
    drop(raw);
    if painter.broken() {
        eprintln!(
            "tiles: the screen model failed after a resize, so cards stopped drawing; the harness was not affected"
        );
    }
    let status = child.wait()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::os::fd::AsFd as _;

    use super::collect_replies;

    #[test]
    fn keys_sent_with_the_reply_stay_for_the_next_reader() {
        let (mut reader, mut writer) = std::io::pipe().unwrap();
        writer.write_all(b"\x1b_Gi=31;OK\x1b\\").unwrap();
        writer.write_all(b"\x1b[?62;4cls\r").unwrap();
        let replies = collect_replies(reader.as_fd());
        assert!(replies.kitty_graphics && replies.sixel && replies.complete);
        drop(writer);
        let mut later = Vec::new();
        reader.read_to_end(&mut later).unwrap();
        assert_eq!(later, b"ls\r");
    }

    #[test]
    fn a_probe_with_no_terminal_names_dev_tty_and_why() {
        let file = std::env::temp_dir().join(format!("tiles-not-a-tty-{}", std::process::id()));
        std::fs::write(&file, b"").unwrap();
        let err = super::Tty::open(&file).err().unwrap();
        assert!(err.ends_with("is not a terminal"), "{err}");
        let gone = std::path::Path::new("/nonexistent/tiles-tty");
        let err = super::Tty::open(gone).err().unwrap();
        assert!(
            err.starts_with("/nonexistent/tiles-tty cannot be opened: "),
            "{err}"
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn dev_tty_is_asked_only_beside_a_terminal() {
        let gone = std::path::Path::new("/nonexistent/tiles-tty");
        // An agent's run: neither is a terminal, and nothing is opened.
        let err = super::Probe::beside((false, false), gone).err().unwrap();
        assert_eq!(err, "stdin and stdout are not terminals");
        // A report piped from a shell, or read from a file in one.
        for stdio in [(true, false), (false, true)] {
            let err = super::Probe::beside(stdio, gone).err().unwrap();
            assert!(
                err.starts_with(
                    "stdin or stdout is not a terminal and /nonexistent/tiles-tty cannot be opened: "
                ),
                "{stdio:?}: {err}"
            );
        }
    }

    #[test]
    fn a_silent_terminal_times_out() {
        let (reader, _writer) = std::io::pipe().unwrap();
        assert!(!collect_replies(reader.as_fd()).complete);
    }

    #[test]
    fn keys_typed_after_the_query_reach_the_next_reader() {
        let (mut reader, mut writer) = std::io::pipe().unwrap();
        writer.write_all(b"\x1b[?62c").unwrap();
        assert!(collect_replies(reader.as_fd()).complete);
        writer.write_all(b"hello").unwrap();
        drop(writer);
        let mut later = Vec::new();
        reader.read_to_end(&mut later).unwrap();
        assert_eq!(later, b"hello");
    }
}
