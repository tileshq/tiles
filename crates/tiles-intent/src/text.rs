//! Plain text rules (ADR-009): the one sanitizer for text Tiles prints, and the
//! check for text Tiles accepts.
//!
//! A label, title or data value can carry escape sequences that write the
//! clipboard (OSC 52), forge hyperlinks (OSC 8), move the cursor or reorder text
//! with bidi overrides. Accepted text must hold none of them; printed text has
//! them removed.

/// Whether `c` can change what a terminal shows or does, or hide text from
/// the person reading it: C0 and C1 controls, DEL, ESC, the Unicode line and
/// paragraph separators, and every format character ([`is_format_char`]).
/// Input and output share this one list. The one character an intent may
/// hold from it is a zero width joiner between two emoji ([`first_refused`]),
/// which the card draws and printed text drops.
#[must_use]
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || is_format_char(c)
}

/// Whether `c` is a format character (Unicode category Cf). Format characters
/// render as nothing or reorder what is around them: the bidi embedding,
/// override, isolate and mark controls, zero-width spaces and joiners, the
/// word joiner and invisible operators, the byte order mark, the soft hyphen,
/// interlinear annotation marks, and the tag characters (U+E0000 to
/// U+E007F), which spell out text a model reads and a person cannot see.
#[must_use]
pub fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// `text` without its format characters, for text Tiles fetches rather than
/// takes from the model (Omni cells): a zero width non-joiner in a Persian
/// word or a soft hyphen goes, and the chart still draws. A zero width joiner
/// inside an emoji sequence, such as 👩‍💻, stays, so the emoji draws whole.
#[must_use]
pub fn strip_format(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|&(i, &c)| !is_format_char(c) || (c == '\u{200D}' && joins_emoji(&chars, i)))
        .map(|(_, &c)| c)
        .collect()
}

/// Whether the joiner at `chars[at]` sits between two emoji, the first
/// perhaps followed by the emoji presentation selector.
fn joins_emoji(chars: &[char], at: usize) -> bool {
    let before = chars[..at].iter().rev().find(|&&c| c != '\u{FE0F}');
    before.is_some_and(|&c| pictographic(c)) && chars.get(at + 1).is_some_and(|&c| pictographic(c))
}

/// Whether `c` is in a block of emoji that join into sequences: the
/// pictographs and symbols, dingbats, arrows and the emoji planes.
fn pictographic(c: char) -> bool {
    matches!(
        c,
        '\u{00A9}'
            | '\u{00AE}'
            | '\u{203C}'
            | '\u{2049}'
            | '\u{2122}'
            | '\u{2139}'
            | '\u{2194}'..='\u{21AA}'
            | '\u{231A}'..='\u{23FF}'
            | '\u{24C2}'
            | '\u{25AA}'..='\u{27BF}'
            | '\u{2934}'..='\u{2935}'
            | '\u{2B05}'..='\u{2B55}'
            | '\u{3030}'
            | '\u{303D}'
            | '\u{3297}'
            | '\u{3299}'
            | '\u{1F000}'..='\u{1FAFF}'
    )
}

/// The first character of `text` an intent refuses, if any: an unsafe
/// character ([`is_unsafe_char`]) other than a zero width joiner between
/// two emoji, such as the one in 👩🏽‍💻, which [`strip_format`] keeps in
/// fetched text too. A joiner anywhere else is refused.
#[must_use]
pub fn first_refused(text: &str) -> Option<char> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .find(|&(i, &c)| is_unsafe_char(c) && !(c == '\u{200D}' && joins_emoji(&chars, i)))
        .map(|(_, &c)| c)
}

/// The character index of the first unsafe character in `text`, if any.
#[must_use]
pub fn first_unsafe(text: &str) -> Option<usize> {
    text.chars().position(is_unsafe_char)
}

/// `text` made safe to print: tabs and line breaks become one space, every
/// other unsafe character is removed.
#[must_use]
pub fn sanitize(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' | '\u{2028}' | '\u{2029}' => Some(' '),
            c if is_unsafe_char(c) => None,
            c => Some(c),
        })
        .collect()
}

/// [`sanitize`], then cut to at most `max_chars` characters, ending in `…`
/// when cut.
#[must_use]
pub fn sanitize_truncated(text: &str, max_chars: usize) -> String {
    let clean = sanitize(text);
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut cut: String = clean.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::{
        first_refused, first_unsafe, is_format_char, is_unsafe_char, sanitize, sanitize_truncated,
        strip_format,
    };

    #[test]
    fn terminal_escapes_are_unsafe() {
        for c in ['\u{1b}', '\u{7}', '\u{7f}', '\u{9b}', '\u{9d}', '\u{0}'] {
            assert!(is_unsafe_char(c), "{:?}", c as u32);
        }
    }

    #[test]
    fn bidi_controls_are_unsafe() {
        for c in ['\u{202E}', '\u{2066}', '\u{2069}', '\u{200F}', '\u{061C}'] {
            assert!(is_unsafe_char(c), "{:?}", c as u32);
        }
    }

    // The security review's F6: an invisible "IGNORE" in tag characters
    // passed every check and reached the model.
    #[test]
    fn tag_characters_are_unsafe_and_stripped() {
        let hidden = "Acme\u{E0049}\u{E0047}\u{E004E}\u{E004F}\u{E0052}\u{E0045}";
        assert_eq!(first_unsafe(hidden), Some(4));
        assert_eq!(sanitize(hidden), "Acme");
        assert!(is_unsafe_char('\u{E0001}') && is_unsafe_char('\u{E007F}'));
        assert!(!is_unsafe_char('\u{E0100}'), "variation selectors stay");
    }

    // The security review's F6: inline input refused zero-width characters
    // that Omni cells and printed text kept. Every format character is now
    // unsafe on both sides.
    #[test]
    fn every_format_character_is_unsafe() {
        for c in [
            '\u{00AD}',
            '\u{200B}',
            '\u{200C}',
            '\u{200D}',
            '\u{2060}',
            '\u{2064}',
            '\u{206F}',
            '\u{FEFF}',
            '\u{FFF9}',
            '\u{1D173}',
            '\u{13430}',
            '\u{1343F}',
            '\u{E0001}',
        ] {
            assert!(is_unsafe_char(c), "{:?}", c as u32);
        }
        assert_eq!(sanitize("a\u{200B}b\u{FEFF}c\u{00AD}d"), "abcd");
    }

    #[test]
    fn line_separators_are_unsafe_but_not_format_characters() {
        for c in ['\u{2028}', '\u{2029}'] {
            assert!(is_unsafe_char(c) && !is_format_char(c), "{:?}", c as u32);
        }
        assert!(!is_format_char('\u{1b}'));
    }

    #[test]
    fn stripping_drops_format_characters_but_joins_emoji() {
        assert_eq!(strip_format("نمی\u{200C}خواهم"), "نمیخواهم");
        assert_eq!(strip_format("Acme\u{E0049}\u{E0047}\u{00AD}"), "Acme");
        assert_eq!(strip_format("👩\u{200D}💻"), "👩\u{200D}💻");
        assert_eq!(strip_format("❤\u{FE0F}\u{200D}🔥"), "❤\u{FE0F}\u{200D}🔥");
        assert_eq!(strip_format("a\u{200D}b"), "ab");
        assert_eq!(strip_format("👩\u{200D}"), "👩");
        assert_eq!(strip_format("\u{200D}💻"), "💻");
        assert_eq!(strip_format("👩\u{200B}\u{200D}💻"), "👩💻");
        assert_eq!(strip_format("tab\tline\u{1b}"), "tab\tline\u{1b}");
    }

    #[test]
    fn an_intent_takes_a_joiner_only_between_two_emoji() {
        assert_eq!(first_refused("👩🏽\u{200D}💻 Priya"), None);
        assert_eq!(first_refused("❤\u{FE0F}\u{200D}🔥"), None);
        assert_eq!(first_refused("a\u{200D}b"), Some('\u{200D}'));
        assert_eq!(first_refused("👩\u{200D}"), Some('\u{200D}'));
        assert_eq!(first_refused("\u{200D}💻"), Some('\u{200D}'));
        assert_eq!(first_refused("👩\u{200D}\u{200D}💻"), Some('\u{200D}'));
        assert_eq!(first_refused("👩\u{200B}💻"), Some('\u{200B}'));
        // Printed text keeps none of them.
        assert_eq!(sanitize("👩🏽\u{200D}💻"), "👩🏽💻");
    }

    #[test]
    fn ordinary_text_is_safe() {
        assert_eq!(first_unsafe("Signups dipped in W3 · €1.2k — 日本"), None);
    }

    #[test]
    fn sanitize_removes_osc_52_and_osc_8() {
        let hostile = "a\u{1b}]52;c;ZXZpbA==\u{7}b\u{1b}]8;;https://x.test\u{1b}\\c";
        let clean = sanitize(hostile);
        assert!(clean.chars().all(|c| !is_unsafe_char(c)));
        assert!(!clean.contains('\u{1b}'));
        assert!(clean.starts_with('a'));
    }

    #[test]
    fn sanitize_turns_line_breaks_into_spaces() {
        assert_eq!(sanitize("one\ntwo\r\tthree"), "one two  three");
    }

    #[test]
    fn first_unsafe_counts_characters_not_bytes() {
        assert_eq!(first_unsafe("€€\u{1b}"), Some(2));
    }

    #[test]
    fn truncation_counts_characters_and_marks_the_cut() {
        assert_eq!(sanitize_truncated("abcdef", 4), "abc…");
        assert_eq!(sanitize_truncated("abcd", 4), "abcd");
        assert_eq!(sanitize_truncated("ééééé", 3), "éé…");
    }
}
