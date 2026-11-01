//! The step list `tiles setup` prints (ADR-060): one aligned line per step,
//! with a coloured mark in a terminal and the same words without it anywhere
//! else. Every word on a line goes through [`crate::sanitize`], so the only
//! escape sequences on a line are its mark's own.

use crate::sanitize;

/// The most characters in one line's state: enough for the two paths of a
/// command, so a command is never cut.
const LONGEST: usize = 4096;

/// The most characters in a label.
const LABEL: usize = 40;

/// How a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Finished, or nothing was left to do.
    Done,
    /// Not done yet: it waits for an answer, or this is a dry run.
    Pending,
    /// Left undone, and it needs nothing.
    Skipped,
    /// Left for the user; the line says what to run.
    Attention,
    /// It failed; the line says why and what to run.
    Failed,
}

impl Mark {
    fn glyph(self) -> &'static str {
        match self {
            Mark::Done => "\x1b[32m\u{2713}\x1b[0m",
            Mark::Pending => "\x1b[2m\u{b7}\x1b[0m",
            Mark::Skipped => "\x1b[2m\u{2013}\x1b[0m",
            Mark::Attention => "\x1b[33m!\x1b[0m",
            Mark::Failed => "\x1b[31m\u{2717}\x1b[0m",
        }
    }
}

/// Whether lines carry their marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    marks: bool,
}

impl Style {
    /// Marks only when stdout is a terminal and `NO_COLOR` is unset.
    #[must_use]
    pub fn new(stdout_is_terminal: bool, no_color: bool) -> Self {
        Self {
            marks: stdout_is_terminal && !no_color,
        }
    }

    /// One step: its mark, its label padded to `width`, and its state in
    /// parentheses.
    #[must_use]
    pub fn step(self, mark: Mark, label: &str, width: usize, state: &str) -> String {
        let label = sanitize::line(label, LABEL);
        let state = sanitize::line(state, LONGEST);
        if self.marks {
            format!("  {} {label:<width$}  ({state})", mark.glyph())
        } else {
            format!("  {label:<width$}  ({state})")
        }
    }

    /// A line of detail under a step, for `--verbose`.
    #[must_use]
    pub fn detail(self, text: &str) -> String {
        format!("      {}", sanitize::line(text, LONGEST))
    }
}

#[cfg(test)]
mod tests {
    use super::{Mark, Style};

    #[test]
    fn a_terminal_gets_a_coloured_mark_and_anywhere_else_the_same_words() {
        let tty = Style::new(true, false);
        assert_eq!(
            tty.step(Mark::Done, "Pi", 6, "configured"),
            "  \x1b[32m\u{2713}\x1b[0m Pi      (configured)"
        );
        for plain in [Style::new(false, false), Style::new(true, true)] {
            assert_eq!(
                plain.step(Mark::Done, "Pi", 6, "configured"),
                "  Pi      (configured)"
            );
        }
    }
}
