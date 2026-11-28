//! The hostile-text corpus (`fixtures/hostile/text.json`) through every text
//! path this crate owns (ADR-009): nothing that can drive a terminal comes out.

use tiles_intent::text::is_unsafe_char;
use tiles_term::style::{Mark, Style};
use tiles_term::{block, detect, fallback, pane, sanitize};

fn corpus() -> Vec<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

fn plain(s: &str) -> bool {
    !s.chars().any(is_unsafe_char)
}

#[test]
fn the_corpus_holds_every_kind_of_escape() {
    let all = corpus().concat();
    for c in [
        '\u{1b}', '\u{7}', '\u{9b}', '\u{9d}', '\u{202e}', '\u{2066}', '\u{2028}', '\0', '\u{7f}',
    ] {
        assert!(all.contains(c), "{:?}", c as u32);
    }
}

#[test]
fn sanitized_text_is_plain() {
    for e in corpus() {
        assert!(plain(&sanitize::line(&e, 200)), "{e:?}");
        assert!(sanitize::lines(&e).lines().all(plain), "{e:?}");
    }
}

#[test]
fn the_placeholder_block_and_the_summary_are_plain() {
    for e in corpus() {
        let lines = block::lines("a1b2c3", 3, &e, 80);
        assert!(lines.iter().all(|l| plain(l)), "{e:?}");
        let summary = fallback::summary(&e, std::slice::from_ref(&e), &e);
        assert!(summary.iter().all(|l| plain(l)), "{e:?}");
    }
}

#[test]
fn a_step_line_holds_only_its_own_mark_sequences() {
    for e in corpus() {
        let words = Style::new(false, false);
        assert!(plain(&words.step(Mark::Failed, &e, 12, &e)), "{e:?}");
        assert!(plain(&words.detail(&e)), "{e:?}");
        let line = Style::new(true, false).step(Mark::Failed, &e, 12, &e);
        let rest = line.strip_prefix("  \x1b[31m\u{2717}\x1b[0m ").unwrap();
        assert!(plain(rest), "{e:?}");
    }
}

#[test]
fn the_pane_status_line_holds_only_its_own_moves() {
    for e in corpus() {
        let line = pane::status(23, 80, &e);
        let rest = line.strip_prefix("\x1b[24;1H\x1b[2K").unwrap();
        assert!(plain(rest), "{e:?}");
    }
}

/// Whether every escape in `s` is one of the coloured status line's own: a
/// cursor move, an erase or a colour (`CSI` digits and `;`, then `H`, `K` or
/// `m`), and the rest is plain.
fn only_own_sequences(s: &str) -> bool {
    let mut rest = s;
    let mut text = String::new();
    while let Some(at) = rest.find('\x1b') {
        text.push_str(&rest[..at]);
        let seq = &rest[at + 1..];
        let Some(body) = seq.strip_prefix('[') else {
            return false;
        };
        let end = body.find(|c: char| !(c.is_ascii_digit() || c == ';'));
        let Some(end) = end.filter(|&e| matches!(body.as_bytes()[e], b'H' | b'K' | b'm')) else {
            return false;
        };
        rest = &body[end + 1..];
    }
    text.push_str(rest);
    plain(&text)
}

#[test]
fn the_coloured_pane_status_line_holds_only_its_own_sequences() {
    for e in corpus() {
        let span = pane::Span {
            bg: Some([1, 2, 3]),
            bold: true,
            ..pane::Span::new(&e, [4, 5, 6])
        };
        let line = pane::status_spans(23, 80, &[span, span], &[span]);
        assert!(only_own_sequences(&line), "{e:?}");
        assert!(plain(&sanitize::run(&e, 200)), "{e:?}");
    }
}

#[test]
fn a_hostile_override_is_refused_in_plain_words() {
    for e in corpus() {
        let env = |k: &str| (k == "TILES_TERMINAL").then(|| e.clone());
        if let Err(bad) = detect::detect(&env, None) {
            assert!(plain(&bad.to_string()), "{e:?}");
        }
        let replies = detect::Replies::parse(format!("\x1bP>|{e}\x1b\\\x1b[?62c").as_bytes());
        assert!(replies.xtversion.as_deref().is_none_or(plain), "{e:?}");
    }
}
