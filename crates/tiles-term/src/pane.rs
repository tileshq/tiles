//! Every byte `tiles pane` writes to the terminal (ADR-063): the screen it
//! takes and gives back, the start-up probe, the cursor moves that place
//! each picture, the frame markers that make an update land at once, and
//! its one status line, which passes through [`crate::sanitize`]. Pictures
//! go through [`crate::kitty`].

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

#[cfg(test)]
mod tests {
    use super::{LEAVE, move_to, status};

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
    fn leaving_turns_off_every_mode_entering_turned_on() {
        for mode in ["1003", "1006", "1016", "2026", "1049"] {
            assert!(LEAVE.contains(&format!("\x1b[?{mode}l")), "{mode}");
        }
        assert!(LEAVE.contains("\x1b[?25h"));
    }
}
