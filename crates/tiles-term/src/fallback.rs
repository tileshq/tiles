//! What a terminal without inline images shows for a card: the title, the
//! values on the card, and where the PNG was saved (ADR-004). Every line goes
//! through the one sanitizer.

/// Most value lines shown before the rest are counted.
pub const MAX_VALUE_LINES: usize = 12;

/// The text summary of a card, line by line.
#[must_use]
pub fn summary(title: &str, values: &[String], png: &str) -> Vec<String> {
    let mut out = vec![format!("▍ {}", crate::sanitize::line(title, 90))];
    for v in values.iter().take(MAX_VALUE_LINES) {
        out.push(format!("  {}", crate::sanitize::line(v, 100)));
    }
    if values.len() > MAX_VALUE_LINES {
        out.push(format!("  … {} more", values.len() - MAX_VALUE_LINES));
    }
    out.push(format!("  PNG: {}", crate::sanitize::line(png, 300)));
    out
}

#[cfg(test)]
mod tests {
    use super::{MAX_VALUE_LINES, summary};

    #[test]
    fn summary_caps_values_and_names_the_png() {
        let values: Vec<String> = (0..20).map(|i| format!("W{i}: {i}")).collect();
        let s = summary("Signups dipped", &values, "/tmp/c.png");
        assert_eq!(s[0], "▍ Signups dipped");
        assert_eq!(s.len(), 1 + MAX_VALUE_LINES + 2);
        assert_eq!(s[s.len() - 2], "  … 8 more");
        assert_eq!(s.last().unwrap(), "  PNG: /tmp/c.png");
    }

    #[test]
    fn summary_is_plain_text() {
        let s = summary("\x1b[2Jt", &["v\x1b]52;c;x\x07".into()], "/p\u{2066}");
        assert!(
            s.iter()
                .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char))
        );
    }
}
