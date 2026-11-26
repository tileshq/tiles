//! What the terminal sends `tiles pane` while the pane owns the screen
//! (ADR-063): SGR mouse reports (modes 1003 and 1006, in pixels under mode
//! 1016), a few keys, and the replies to the pane's start-up probe.
//!
//! The parser is total: it reads any byte sequence one byte at a time,
//! returns an [`Event`] for what it recognises and drops the rest, keeps at
//! most [`MAX_PARAMS`] bytes of one sequence, and never panics. Feeding the
//! same bytes in any split gives the same events. `fuzz/fuzz_targets/
//! pane_input.rs` holds it to that.

/// Most parameter and intermediate bytes one control sequence keeps; a
/// longer one is read to its end and dropped.
pub const MAX_PARAMS: usize = 32;

/// Most bytes of a string sequence (OSC, DCS, APC, PM, SOS) before the
/// parser stops waiting for its end and reads keys again.
pub const MAX_STRING: usize = 1 << 16;

/// One thing the terminal said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A mouse report.
    Mouse(Mouse),
    /// A key.
    Key(Key),
    /// The reply to the pixel mouse query (`CSI ? 1016 $ p`): whether the
    /// terminal knows mode 1016.
    PixelMouse(bool),
    /// One cell's size in pixels, from `CSI 16 t`.
    CellSize {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The device attributes reply, which ends the start-up probe.
    DeviceAttributes,
}

/// A mouse report, at a 0-based position: pixels when mode 1016 is on,
/// cells otherwise. The parser cannot tell which; the pane knows what it
/// turned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    /// What happened.
    pub kind: MouseKind,
    /// Column or pixel from the left.
    pub x: u32,
    /// Row or pixel from the top.
    pub y: u32,
}

/// What a mouse report says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    /// The pointer moved, with or without a button held.
    Move,
    /// A button went down: 0 left, 1 middle, 2 right.
    Press(u8),
    /// A button came up.
    Release(u8),
    /// The wheel turned up.
    ScrollUp,
    /// The wheel turned down.
    ScrollDown,
}

/// A key the pane acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable ASCII character.
    Char(char),
    /// Control-C, which raw mode delivers as a byte instead of a signal.
    Interrupt,
    /// Control-D.
    EndOfInput,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Csi,
    Ss3,
    /// Inside an OSC, DCS, APC, PM or SOS string; `escape` when the last
    /// byte was ESC, which a `\` makes the string's end.
    Text {
        escape: bool,
        seen: usize,
    },
}

/// The pane's input parser.
#[derive(Debug, Clone)]
pub struct Parser {
    state: State,
    params: [u8; MAX_PARAMS],
    len: usize,
    overflow: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// A parser at the start of input.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: [0; MAX_PARAMS],
            len: 0,
            overflow: false,
        }
    }

    /// Reads `bytes` and appends the events they complete to `out`. A
    /// sequence cut between two calls finishes in the second.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Event>) {
        for &b in bytes {
            if let Some(event) = self.step(b) {
                out.push(event);
            }
        }
    }

    /// [`Parser::feed`] into a new list.
    #[must_use]
    pub fn parse(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        self.feed(bytes, &mut out);
        out
    }

    fn start_sequence(&mut self, state: State) {
        self.state = state;
        self.len = 0;
        self.overflow = false;
    }

    fn step(&mut self, b: u8) -> Option<Event> {
        // Control-C always reaches the pane, whatever the parser was
        // reading, so a stream that never ends a sequence cannot keep the
        // user from leaving.
        if b == 0x03 {
            self.state = State::Ground;
            return Some(Event::Key(Key::Interrupt));
        }
        match self.state {
            State::Ground => self.ground(b),
            State::Escape => {
                match b {
                    b'[' => self.start_sequence(State::Csi),
                    b'O' => self.start_sequence(State::Ss3),
                    b']' | b'P' | b'_' | b'^' | b'X' => {
                        self.state = State::Text {
                            escape: false,
                            seen: 0,
                        };
                    }
                    0x1b => {}
                    _ => self.state = State::Ground,
                }
                None
            }
            State::Csi => self.csi(b),
            State::Ss3 => {
                self.state = State::Ground;
                match b {
                    b'A' => Some(Event::Key(Key::Up)),
                    b'B' => Some(Event::Key(Key::Down)),
                    0x1b => {
                        self.state = State::Escape;
                        None
                    }
                    _ => None,
                }
            }
            State::Text { escape, seen } => {
                self.state = match b {
                    b'\\' if escape => State::Ground,
                    0x07 | 0x18 | 0x1a => State::Ground,
                    _ if seen >= MAX_STRING => State::Ground,
                    _ => State::Text {
                        escape: b == 0x1b,
                        seen: seen + 1,
                    },
                };
                None
            }
        }
    }

    fn ground(&mut self, b: u8) -> Option<Event> {
        match b {
            0x1b => {
                self.state = State::Escape;
                None
            }
            0x04 => Some(Event::Key(Key::EndOfInput)),
            0x20..=0x7e => Some(Event::Key(Key::Char(char::from(b)))),
            _ => None,
        }
    }

    fn csi(&mut self, b: u8) -> Option<Event> {
        match b {
            0x20..=0x3f => {
                if self.len < MAX_PARAMS {
                    self.params[self.len] = b;
                    self.len += 1;
                } else {
                    self.overflow = true;
                }
                None
            }
            0x40..=0x7e => {
                self.state = State::Ground;
                if self.overflow {
                    return None;
                }
                let len = self.len.min(MAX_PARAMS);
                interpret(&self.params[..len], b)
            }
            0x1b => {
                self.state = State::Escape;
                None
            }
            0x18 | 0x1a | 0x80..=0xff => {
                self.state = State::Ground;
                None
            }
            // Other C0 controls inside a sequence are executed by a
            // terminal and leave the sequence going; the pane drops them.
            _ => None,
        }
    }
}

/// Up to `N` numbers separated by `;`, each at most `u32::MAX`; `None`
/// for any other byte, an empty or overlong number, or more than `N`.
fn numbers<const N: usize>(bytes: &[u8]) -> Option<([u32; N], usize)> {
    let mut out = [0u32; N];
    let mut count = 0;
    for part in bytes.split(|&b| b == b';') {
        if count == N || part.is_empty() {
            return None;
        }
        let mut n: u32 = 0;
        for &d in part {
            if !d.is_ascii_digit() {
                return None;
            }
            n = n.checked_mul(10)?.checked_add(u32::from(d - b'0'))?;
        }
        out[count] = n;
        count += 1;
    }
    Some((out, count))
}

fn interpret(params: &[u8], last: u8) -> Option<Event> {
    match (params.first(), last) {
        (Some(b'<'), b'M' | b'm') => mouse(&params[1..], last == b'm'),
        (Some(b'?'), b'y') => {
            let body = params[1..].strip_suffix(b"$")?;
            let (n, 2) = numbers::<2>(body)? else {
                return None;
            };
            (n[0] == 1016).then_some(Event::PixelMouse(matches!(n[1], 1..=3)))
        }
        (Some(b'?'), b'c') => Some(Event::DeviceAttributes),
        (_, b't') => {
            let (n, 3) = numbers::<3>(params)? else {
                return None;
            };
            let (height, width) = (u16::try_from(n[1]).ok()?, u16::try_from(n[2]).ok()?);
            (n[0] == 6 && height > 0 && width > 0).then_some(Event::CellSize { width, height })
        }
        (None, b'A') => Some(Event::Key(Key::Up)),
        (None, b'B') => Some(Event::Key(Key::Down)),
        (Some(_), b'~') => match numbers::<1>(params)? {
            ([5], 1) => Some(Event::Key(Key::PageUp)),
            ([6], 1) => Some(Event::Key(Key::PageDown)),
            _ => None,
        },
        _ => None,
    }
}

/// An SGR mouse report's `Cb ; Cx ; Cy`, ended by `M` or (`release`) `m`.
fn mouse(params: &[u8], release: bool) -> Option<Event> {
    let (n, 3) = numbers::<3>(params)? else {
        return None;
    };
    let (code, x, y) = (n[0], n[1].saturating_sub(1), n[2].saturating_sub(1));
    // Buttons 8 to 11 (code 128 and over) are extra buttons the pane does
    // not use.
    if code >= 128 {
        return None;
    }
    let button = u8::try_from(code & 3).ok()?;
    let kind = if code & 64 != 0 {
        match button {
            0 => MouseKind::ScrollUp,
            1 => MouseKind::ScrollDown,
            _ => return None,
        }
    } else if code & 32 != 0 {
        MouseKind::Move
    } else if button == 3 {
        // No button: a plain motion report under the X10 encoding.
        MouseKind::Move
    } else if release {
        MouseKind::Release(button)
    } else {
        MouseKind::Press(button)
    };
    Some(Event::Mouse(Mouse { kind, x, y }))
}

#[cfg(test)]
mod tests {
    use super::{Event, Key, MAX_PARAMS, MAX_STRING, Mouse, MouseKind, Parser};

    fn parse(bytes: &[u8]) -> Vec<Event> {
        Parser::new().parse(bytes)
    }

    fn mouse(kind: MouseKind, x: u32, y: u32) -> Event {
        Event::Mouse(Mouse { kind, x, y })
    }

    #[test]
    fn sgr_reports_name_the_button_and_a_zero_based_place() {
        assert_eq!(
            parse(b"\x1b[<35;10;4M\x1b[<0;1;1M\x1b[<0;1;1m\x1b[<2;5;6M"),
            [
                mouse(MouseKind::Move, 9, 3),
                mouse(MouseKind::Press(0), 0, 0),
                mouse(MouseKind::Release(0), 0, 0),
                mouse(MouseKind::Press(2), 4, 5),
            ]
        );
        assert_eq!(
            parse(b"\x1b[<64;3;3M\x1b[<65;3;3M\x1b[<66;3;3M\x1b[<32;800;600M"),
            [
                mouse(MouseKind::ScrollUp, 2, 2),
                mouse(MouseKind::ScrollDown, 2, 2),
                mouse(MouseKind::Move, 799, 599),
            ]
        );
    }

    #[test]
    fn a_report_cut_anywhere_reads_the_same() {
        let whole = b"x\x1b[<35;1200;700M\x1b[?1016;1$y\x1b]11;rgb:0/0/0\x1b\\q";
        let expected = parse(whole);
        assert_eq!(expected.len(), 4);
        for cut in 0..whole.len() {
            let mut p = Parser::new();
            let mut got = p.parse(&whole[..cut]);
            got.extend(p.parse(&whole[cut..]));
            assert_eq!(got, expected, "cut at {cut}");
        }
    }

    #[test]
    fn probe_replies_are_read() {
        assert_eq!(
            parse(b"\x1b[?1016;2$y\x1b[?1016;0$y\x1b[6;32;16t\x1b[?62;22c"),
            [
                Event::PixelMouse(true),
                Event::PixelMouse(false),
                Event::CellSize {
                    width: 16,
                    height: 32
                },
                Event::DeviceAttributes,
            ]
        );
        // Another mode's reply, a zero cell and a window size say nothing.
        assert_eq!(parse(b"\x1b[?2026;2$y\x1b[6;0;16t\x1b[4;900;1600t"), []);
    }

    #[test]
    fn keys_are_read_and_other_bytes_dropped() {
        assert_eq!(
            parse(b"q\x03\x04\x1b[A\x1bOB\x1b[5~\x1b[6~\r\n\x7f\xc3\xa9"),
            [
                Event::Key(Key::Char('q')),
                Event::Key(Key::Interrupt),
                Event::Key(Key::EndOfInput),
                Event::Key(Key::Up),
                Event::Key(Key::Down),
                Event::Key(Key::PageUp),
                Event::Key(Key::PageDown),
            ]
        );
    }

    #[test]
    fn malformed_reports_are_dropped() {
        for bad in [
            &b"\x1b[<35;10M"[..],
            b"\x1b[<35;10;4;1M",
            b"\x1b[<35;;4M",
            b"\x1b[<35;99999999999;4M",
            b"\x1b[<3:1;1;1M",
            b"\x1b[<128;1;1M",
            b"\x1b[<67;1;1M",
            b"\x1b[?1016$y",
        ] {
            assert_eq!(parse(bad), [], "{bad:?}");
        }
    }

    #[test]
    fn an_overlong_sequence_is_read_to_its_end_and_dropped() {
        let mut long = b"\x1b[<".to_vec();
        long.extend(std::iter::repeat_n(b'1', MAX_PARAMS * 4));
        long.extend(b";1;1Mq");
        assert_eq!(parse(&long), [Event::Key(Key::Char('q'))]);
    }

    #[test]
    fn strings_are_skipped_and_an_endless_one_gives_up() {
        assert_eq!(
            parse(b"\x1b_Gi=1;OK\x1b\\\x1bP1$r0m\x1b\\\x1b]52;c;cQ==\x07q"),
            [Event::Key(Key::Char('q'))]
        );
        let mut endless = b"\x1b]".to_vec();
        endless.extend(std::iter::repeat_n(b'q', MAX_STRING + 1));
        endless.push(b'z');
        assert_eq!(parse(&endless), [Event::Key(Key::Char('z'))]);
    }

    #[test]
    fn control_c_ends_any_sequence() {
        assert_eq!(
            parse(b"\x1b]never ends\x03q"),
            [Event::Key(Key::Interrupt), Event::Key(Key::Char('q'))]
        );
        assert_eq!(parse(b"\x1b[<35;1\x03"), [Event::Key(Key::Interrupt)]);
    }
}
