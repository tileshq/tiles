//! The hostile-text corpus (`fixtures/hostile/text.json`) through every text
//! path this crate owns (ADR-009): nothing that can drive a terminal comes out.

use tiles_intent::text::is_unsafe_char;
use tiles_term::{block, detect, fallback, sanitize};

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
