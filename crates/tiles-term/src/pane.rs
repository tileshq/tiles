//! Every byte `tiles pane` writes to the terminal (ADR-063): the screen it
//! takes and gives back, the start-up probe, the cursor moves that place
//! each picture, the frame markers that make an update land at once, and
//! its one status line, which passes through [`crate::sanitize`]. Pictures
//! go through [`crate::kitty`].

use std::fmt::Write as _;

use crate::sanitize;

/// Takes the screen: the alternate screen, cleared, the cursor hidden.
pub const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[2J";

/// Turns on mouse reports: every pointer motion (1003), in SGR form (1006).
pub const MOUSE: &str = "\x1b[?1003h\x1b[?1006h";

/// Asks whether the terminal knows pixel mouse reports (DECRQM for mode
/// 1016) and how large one cell is (`CSI 16 t`), then for its device
/// attributes, whose reply every terminal sends and so ends the wait.
pub const PROBE: &str = "\x1b[?1016$p\x1b[16t\x1b[c";

/// Asks for one cell's size in pixels again, after a resize.
pub const CELL_SIZE: &str = "\x1b[16t";

/// Turns on pixel mouse reports (mode 1016).
pub const PIXEL_MOUSE: &str = "\x1b[?1016h";

/// Gives the screen back: ends any update in progress, turns every mouse
/// mode off, deletes every picture on the pane's screen with its data,
/// shows the cursor and leaves the alternate screen. Safe to send twice.
pub const LEAVE: &str =
    "\x1b[?2026l\x1b[?1016l\x1b[?1006l\x1b[?1003l\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?25h\x1b[?1049l";

/// Starts an update the terminal shows only when [`END_UPDATE`] arrives
/// (synchronized output, mode 2026), so a picture replaced by another never
/// shows a gap.
pub const BEGIN_UPDATE: &str = "\x1b[?2026h";

/// Ends an update started by [`BEGIN_UPDATE`].
pub const END_UPDATE: &str = "\x1b[?2026l";

/// Clears the screen and deletes every picture on it with its data, for a
/// redraw from nothing after a resize.
pub const CLEAR: &str = "\x1b_Ga=d,d=A,q=2\x1b\\\x1b[2J";

/// Moves the cursor to 0-based `row` and `col`.
#[must_use]
pub fn move_to(row: u16, col: u16) -> String {
    format!("\x1b[{};{}H", u32::from(row) + 1, u32::from(col) + 1)
}

/// The status line on 0-based `row`: the line cleared, then `text` as one
/// printable line of at most `cols` characters.
#[must_use]
pub fn status(row: u16, cols: u16, text: &str) -> String {
    format!(
        "{}\x1b[2K{}",
        move_to(row, 0),
        sanitize::line(text, usize::from(cols))
    )
}

/// One run of the status line: its text and its colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span<'a> {
    /// The text, sanitized when the line is built.
    pub text: &'a str,
    /// Its colour, as red, green and blue; the terminal's own when `None`.
    pub fg: Option<[u8; 3]>,
    /// Its background; the terminal's own when `None`.
    pub bg: Option<[u8; 3]>,
    /// Bold.
    pub bold: bool,
}

impl<'a> Span<'a> {
    /// `text` in colour `fg`.
    #[must_use]
    pub fn new(text: &'a str, fg: [u8; 3]) -> Self {
        Self {
            text,
            fg: Some(fg),
            ..Self::default()
        }
    }
}

/// Characters `spans` take once sanitized and cut to `max` in all.
fn width(spans: &[Span<'_>], max: usize) -> usize {
    let mut left = max;
    for s in spans {
        left -= sanitize::run(s.text, left).chars().count().min(left);
    }
    max - left
}

fn push_runs(out: &mut String, spans: &[Span<'_>], mut room: usize) {
    for s in spans {
        if room == 0 {
            break;
        }
        let text = sanitize::run(s.text, room);
        room -= text.chars().count().min(room);
        let mut sgr = Vec::new();
        if s.bold {
            sgr.push("1".to_string());
        }
        if let Some([r, g, b]) = s.fg {
            sgr.push(format!("38;2;{r};{g};{b}"));
        }
        if let Some([r, g, b]) = s.bg {
            sgr.push(format!("48;2;{r};{g};{b}"));
        }
        if sgr.is_empty() {
            out.push_str(&text);
        } else {
            let _ = write!(out, "\x1b[{}m{text}\x1b[0m", sgr.join(";"));
        }
    }
}

/// The status line on 0-based `row` of a `cols`-column screen, in colour
/// (ADR-069): the line cleared, `left` from its first column and `right`
/// against its last but one when both fit with two columns between them,
/// else `left` alone, cut to the line. Each run's text passes through
/// [`crate::sanitize`]; the only control sequences are the cursor moves and
/// each run's own colours, reset after it. The last column stays empty so
/// the line never wraps.
#[must_use]
pub fn status_spans(row: u16, cols: u16, left: &[Span<'_>], right: &[Span<'_>]) -> String {
    let room = usize::from(cols).saturating_sub(1);
    let mut out = format!("{}\x1b[2K", move_to(row, 0));
    let lw = width(left, room);
    push_runs(&mut out, left, room);
    let rw = width(right, room);
    if rw > 0 && lw + 2 + rw <= room {
        let col = u16::try_from(room - rw).unwrap_or(0);
        out.push_str(&move_to(row, col));
        push_runs(&mut out, right, rw);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LEAVE, Span, move_to, status, status_spans};

    #[test]
    fn the_cursor_moves_one_based() {
        assert_eq!(move_to(0, 0), "\x1b[1;1H");
        assert_eq!(move_to(u16::MAX, 4), "\x1b[65536;5H");
    }

    #[test]
    fn the_status_line_is_plain_and_cut() {
        assert_eq!(
            status(23, 8, "Germany\x1b]52;c;eA==\x07 selected"),
            "\x1b[24;1H\x1b[2KGermany…"
        );
    }

    #[test]
    fn the_coloured_status_line_puts_its_keys_at_the_right() {
        let grey = [138, 143, 152];
        let left = [
            Span::new("tiles pane", [247, 248, 248]),
            Span::new(" · a.json", grey),
        ];
        let key = Span {
            bg: Some([28, 29, 33]),
            ..Span::new(" q ", [247, 248, 248])
        };
        let right = [key, Span::new(" quit", grey)];
        let line = status_spans(9, 40, &left, &right);
        assert_eq!(
            line,
            "\x1b[10;1H\x1b[2K\x1b[38;2;247;248;248mtiles pane\x1b[0m\x1b[38;2;138;143;152m · a.json\x1b[0m\
             \x1b[10;32H\x1b[38;2;247;248;248;48;2;28;29;33m q \x1b[0m\x1b[38;2;138;143;152m quit\x1b[0m"
        );
        // Too narrow for both: the left alone, cut short of the last column.
        let line = status_spans(0, 12, &left, &right);
        assert!(
            line.ends_with("tiles pane\x1b[0m\x1b[38;2;138;143;152m…\x1b[0m"),
            "{line:?}"
        );
    }

    #[test]
    fn the_coloured_status_line_strips_escapes_from_its_text() {
        let left = [Span::new("Germany\x1b]52;c;eA==\x07 picked", [1, 2, 3])];
        let line = status_spans(0, 80, &left, &[]);
        assert_eq!(
            line,
            "\x1b[1;1H\x1b[2K\x1b[38;2;1;2;3mGermany]52;c;eA== picked\x1b[0m"
        );
    }

    #[test]
    fn leaving_turns_off_every_mode_entering_turned_on() {
        for mode in ["1003", "1006", "1016", "2026", "1049"] {
            assert!(LEAVE.contains(&format!("\x1b[?{mode}l")), "{mode}");
        }
        assert!(LEAVE.contains("\x1b[?25h"));
    }
}
