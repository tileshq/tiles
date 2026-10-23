//! The one text path to the terminal (ADR-009): everything Tiles prints as
//! text, whether alt text, a summary, an error or the wrapper's own caption,
//! passes through here, which removes C0 and C1 controls, ESC and bidi
//! controls ([`tiles_intent::text::sanitize`]).

use tiles_intent::text::{sanitize, sanitize_truncated};

/// `text` as one printable line of at most `max` characters.
#[must_use]
pub fn line(text: &str, max: usize) -> String {
    sanitize_truncated(text.trim(), max)
}

/// `text` as printable lines: each line is sanitized on its own, so line
/// breaks survive and nothing else does.
#[must_use]
pub fn lines(text: &str) -> String {
    text.lines().map(sanitize).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::{line, lines};

    #[test]
    fn line_strips_escapes_and_cuts() {
        assert_eq!(line("  a\x1b]52;c;eA==\x07b  ", 10), "a]52;c;eA…");
        assert_eq!(line("a\u{202e}b", 10), "ab");
    }

    #[test]
    fn lines_keeps_line_breaks_only() {
        assert_eq!(
            lines("one\x1b[2J\ntwo\r\n\u{9b}three"),
            "one[2J\ntwo\nthree"
        );
    }
}
