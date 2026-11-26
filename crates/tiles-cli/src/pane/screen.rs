//! The terminal while `tiles pane` holds it (ADR-063): raw mode and the
//! alternate screen, given back on every way out, the start-up probe, and
//! the signals that end the pane or resize it.
//!
//! Raw mode turns Control-C into a byte, so the terminal sends no SIGINT;
//! a signal from elsewhere (`kill`, a closed window) arrives through a
//! self-pipe ([`Signals`]) and ends the loop like `q`. A panic gives the
//! screen back from its hook before the message prints. Only SIGKILL leaves
//! the screen as it was, since nothing can run after it.

use std::io::{Read as _, Write as _};
use std::os::fd::AsFd;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tiles_term::input::{Event, Parser};
use tiles_term::pane;

/// The terminal's settings before the pane, for every way out.
static SAVED: OnceLock<Option<rustix::termios::Termios>> = OnceLock::new();
/// Whether the screen was given back already.
static GIVEN_BACK: AtomicBool = AtomicBool::new(false);

/// Gives the screen back once: every mode the pane turned on off, its
/// pictures deleted, the alternate screen left and the terminal's settings
/// restored. Later calls do nothing.
pub fn give_back() {
    if GIVEN_BACK.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(pane::LEAVE.as_bytes());
    let _ = out.flush();
    if let Some(Some(saved)) = SAVED.get() {
        let _ = rustix::termios::tcsetattr(
            rustix::stdio::stdin(),
            rustix::termios::OptionalActions::Now,
            saved,
        );
    }
}

/// The terminal, held until dropped.
pub struct Screen(());

/// What the start-up probe learned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Probed {
    /// The terminal knows pixel mouse reports (mode 1016).
    pub pixels: bool,
    /// One cell in pixels, from `CSI 16 t`.
    pub cell: Option<(u16, u16)>,
}

impl Screen {
    /// Takes the terminal: raw mode, the alternate screen, and a panic hook
    /// that gives both back.
    ///
    /// # Errors
    ///
    /// When standard input or output is not a terminal, or the pane ran
    /// once already in this process.
    pub fn take() -> Result<Self, String> {
        let tty = |fd| rustix::termios::isatty(fd);
        if !tty(rustix::stdio::stdin()) || !tty(rustix::stdio::stdout()) {
            return Err("tiles pane needs a terminal on standard input and output".into());
        }
        let saved = rustix::termios::tcgetattr(rustix::stdio::stdin()).ok();
        if SAVED.set(saved.clone()).is_err() {
            return Err("the pane holds the terminal already".into());
        }
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            give_back();
            hook(info);
        }));
        if let Some(saved) = &saved {
            let mut raw = saved.clone();
            raw.make_raw();
            let _ = rustix::termios::tcsetattr(
                rustix::stdio::stdin(),
                rustix::termios::OptionalActions::Now,
                &raw,
            );
        }
        write(pane::ENTER.as_bytes())?;
        Ok(Self(()))
    }

    /// Asks the terminal about pixel mouse reports and its cell size, and
    /// reads replies until its device attributes arrive or `wait` passes.
    #[allow(clippy::unused_self)] // Holding the screen proves raw mode is on.
    pub fn probe(&self, wait: Duration) -> Probed {
        let mut probed = Probed::default();
        if write(pane::PROBE.as_bytes()).is_err() {
            return probed;
        }
        let mut parser = Parser::new();
        let start = Instant::now();
        while let Some(left) = wait.checked_sub(start.elapsed()) {
            if !readable(left) {
                break;
            }
            let Ok(Some(bytes)) = read() else { break };
            for event in parser.parse(&bytes) {
                match event {
                    Event::PixelMouse(on) => probed.pixels = on,
                    Event::CellSize { width, height } => probed.cell = Some((width, height)),
                    Event::DeviceAttributes => return probed,
                    _ => {}
                }
            }
        }
        probed
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        give_back();
    }
}

/// Writes and flushes `bytes` to the terminal.
///
/// # Errors
///
/// When the terminal is gone.
pub fn write(bytes: &[u8]) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes)
        .and_then(|()| out.flush())
        .map_err(|e| format!("cannot write to the terminal: {e}"))
}

/// Reads what the terminal sent; `None` at the end of input.
///
/// # Errors
///
/// When reading fails.
pub fn read() -> Result<Option<Vec<u8>>, String> {
    let mut buf = vec![0u8; 1 << 16];
    match std::io::stdin().lock().read(&mut buf) {
        Ok(0) => Ok(None),
        Ok(n) => {
            buf.truncate(n);
            Ok(Some(buf))
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(Some(Vec::new())),
        Err(e) => Err(format!("cannot read the terminal: {e}")),
    }
}

/// Whether input arrives within `wait`.
fn readable(wait: Duration) -> bool {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    let Ok(wait) = Timespec::try_from(wait) else {
        return false;
    };
    let stdin = rustix::stdio::stdin();
    matches!(
        poll(&mut [PollFd::from_borrowed_fd(stdin, PollFlags::IN)], Some(&wait)),
        Ok(n) if n > 0
    )
}

/// The terminal's rows and columns, 24 by 80 when it does not say.
#[must_use]
pub fn size() -> (u16, u16) {
    rustix::termios::tcgetwinsize(rustix::stdio::stdout())
        .ok()
        .filter(|w| w.ws_row > 0 && w.ws_col > 0)
        .map_or((24, 80), |w| (w.ws_row, w.ws_col))
}

/// One cell's size from the window size, when the terminal reports pixels.
#[must_use]
pub fn window_cell() -> Option<(u16, u16)> {
    crate::wrap::cell_px_of(rustix::stdio::stdout())
}

/// What woke the loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ready {
    /// A signal asked the pane to end.
    pub quit: bool,
    /// The window changed size.
    pub resize: bool,
    /// The terminal sent input.
    pub input: bool,
}

/// The self-pipes SIGINT, SIGTERM, SIGHUP and SIGQUIT (`quit`) and
/// SIGWINCH (`resize`) write to.
pub struct Signals {
    quit: std::io::PipeReader,
    resize: std::io::PipeReader,
}

impl Signals {
    /// Routes the signals to their pipes.
    ///
    /// # Errors
    ///
    /// When a pipe or a handler cannot be made.
    pub fn register() -> Result<Self, String> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGWINCH};
        let failed = |e: std::io::Error| format!("cannot route signals: {e}");
        let (quit, quit_tx) = std::io::pipe().map_err(failed)?;
        for signal in [SIGINT, SIGTERM, SIGHUP, SIGQUIT] {
            signal_hook::low_level::pipe::register(signal, quit_tx.try_clone().map_err(failed)?)
                .map_err(failed)?;
        }
        let (resize, resize_tx) = std::io::pipe().map_err(failed)?;
        signal_hook::low_level::pipe::register(SIGWINCH, resize_tx).map_err(failed)?;
        Ok(Self { quit, resize })
    }

    /// Blocks until a signal or input arrives, with no timer: an idle pane
    /// wakes for nothing.
    pub fn wait(&self) -> Ready {
        use rustix::event::{PollFd, PollFlags, poll};
        let stdin = rustix::stdio::stdin();
        let mut fds = [
            PollFd::from_borrowed_fd(self.quit.as_fd(), PollFlags::IN),
            PollFd::from_borrowed_fd(self.resize.as_fd(), PollFlags::IN),
            PollFd::from_borrowed_fd(stdin, PollFlags::IN),
        ];
        if poll(&mut fds, None).is_err() {
            return Ready::default();
        }
        let hit = |fd: &PollFd<'_>| !fd.revents().is_empty();
        let ready = Ready {
            quit: hit(&fds[0]),
            resize: hit(&fds[1]),
            input: hit(&fds[2]),
        };
        if ready.resize {
            let mut drain = [0u8; 64];
            let _ = (&self.resize).read(&mut drain);
        }
        ready
    }
}
