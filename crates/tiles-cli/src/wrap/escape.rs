//! Whether a byte stream is between escape sequences: the wrapper paints only
//! there, so its own sequences never land inside one of the harness's
//! (ADR-004, the prototype's first paint rule). It also follows the harness's
//! synchronized-output mode (`CSI ? 2026 h` / `l`).
//!
//! The terminal decodes UTF-8, so a byte from 0x80 to 0x9f is part of a
//! character or an invalid byte, never an 8-bit control: a lone one in text
//! is ignored, as the terminal and `vt100` ignore it, and one inside a
//! character in an OSC or DCS string does not end the string.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    Esc,
    Csi,
    /// OSC, DCS, APC, PM or SOS: a string ended by BEL or ST.
    Str,
    StrEsc,
}

/// Tracks the escape state across chunks.
#[derive(Debug, Clone, Default)]
pub struct Tracker {
    state: State,
    utf8_left: u8,
    csi: Vec<u8>,
    /// The harness has synchronized output on.
    pub harness_sync: bool,
}

impl Tracker {
    /// Feeds bytes through.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.step(b);
        }
    }

    /// Whether the stream is at ground state: no sequence and no UTF-8
    /// character is half written.
    #[must_use]
    pub fn ground(&self) -> bool {
        self.state == State::Ground && self.utf8_left == 0
    }

    /// Follows UTF-8 in text and strings: whether `b` continues a character.
    fn continues_character(&mut self, b: u8) -> bool {
        if self.utf8_left > 0 && (0x80..0xc0).contains(&b) {
            self.utf8_left -= 1;
            return true;
        }
        self.utf8_left = match b {
            0xc0..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf7 => 3,
            _ => 0,
        };
        false
    }

    fn step(&mut self, b: u8) {
        match self.state {
            State::Ground => {
                if !self.continues_character(b) && b == 0x1b {
                    self.state = State::Esc;
                }
            }
            State::Esc => match b {
                b'[' => self.enter_csi(),
                b']' | b'P' | b'_' | b'^' | b'X' => {
                    self.state = State::Str;
                    self.utf8_left = 0;
                }
                // Intermediates keep the sequence open; anything else ends it.
                0x20..=0x2f => {}
                _ => self.state = State::Ground,
            },
            State::Csi => {
                if (0x40..=0x7e).contains(&b) {
                    if (b == b'h' || b == b'l') && self.csi == b"?2026" {
                        self.harness_sync = b == b'h';
                    }
                    self.state = State::Ground;
                } else if self.csi.len() < 32 {
                    self.csi.push(b);
                }
            }
            State::Str => {
                if self.continues_character(b) {
                    return;
                }
                match b {
                    0x07 | 0x9c => {
                        self.state = State::Ground;
                        self.utf8_left = 0;
                    }
                    0x1b => self.state = State::StrEsc,
                    _ => {}
                }
            }
            State::StrEsc => {
                self.state = if b == b'\\' {
                    State::Ground
                } else {
                    State::Str
                };
            }
        }
    }

    fn enter_csi(&mut self) {
        self.state = State::Csi;
        self.csi.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::Tracker;

    fn after(chunks: &[&[u8]]) -> Tracker {
        let mut t = Tracker::default();
        for c in chunks {
            t.feed(c);
        }
        t
    }

    #[test]
    fn complete_sequences_end_at_ground() {
        assert!(after(&[b"hi \x1b[31mred\x1b[0m"]).ground());
        assert!(after(&[b"\x1b]0;title\x07", b"\x1b_Gq=2;AAAA\x1b\\", b"\x1b(B"]).ground());
    }

    #[test]
    fn split_sequences_are_not_ground() {
        assert!(!after(&[b"\x1b"]).ground());
        assert!(!after(&[b"\x1b[3"]).ground());
        assert!(!after(&[b"\x1b]8;;https://x"]).ground());
        assert!(!after(&[b"\x1b]8;;x\x1b"]).ground());
        assert!(after(&[b"\x1b[3", b"1m"]).ground());
    }

    #[test]
    fn a_split_utf8_character_is_not_ground() {
        let e = "é".as_bytes();
        assert!(!after(&[&e[..1]]).ground());
        assert!(after(&[&e[..1], &e[1..]]).ground());
        let wide = "\u{10EEEE}".as_bytes();
        assert!(!after(&[&wide[..3]]).ground());
    }

    // The security review's F10: a lone C1 byte put the tracker in string
    // state until a BEL or ST happened to arrive, and no card was drawn.
    #[test]
    fn a_lone_c1_byte_does_not_stall_the_tracker() {
        for c1 in [0x90u8, 0x98, 0x9b, 0x9d, 0x9e, 0x9f] {
            assert!(after(&[&[b'x', c1, b'y']]).ground(), "{c1:#x}");
        }
    }

    #[test]
    fn a_character_ending_in_0x9c_does_not_end_an_osc_string() {
        // U+1F4DC is F0 9F 93 9C: its last byte is the 8-bit ST.
        let title = "\x1b]0;scroll \u{1F4DC} still the title";
        assert!(!after(&[title.as_bytes()]).ground());
        assert!(after(&[title.as_bytes(), b"\x07"]).ground());
        assert!(
            after(&[b"\x1b]0;t\x9c"]).ground(),
            "a lone ST still ends it"
        );
    }

    #[test]
    fn the_harness_sync_mode_is_followed() {
        assert!(after(&[b"\x1b[?2026h"]).harness_sync);
        assert!(!after(&[b"\x1b[?2026h", b"x\x1b[?2026l"]).harness_sync);
        assert!(!after(&[b"\x1b[?2025h"]).harness_sync);
    }
}
