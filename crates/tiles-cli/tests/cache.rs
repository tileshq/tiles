//! `tiles cache clear` (ADR-028): it removes every saved card with its call
//! and frames, says how many went, and leaves a cache `tiles doctor` still
//! reads. Every line it prints is clean, the hostile corpus included.

use std::path::Path;
use std::process::{Command, Output};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

fn tiles(cache: &Path, args: &[&str]) -> Output {
    Command::new(TILES)
        .args(args)
        .env("TILES_CACHE_DIR", cache)
        .env("TILES_CONFIG_DIR", cache.with_extension("config"))
        .env("TILES_DRAW", "image")
        .env_remove("TILES_SESSION")
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn clean(out: &Output) -> bool {
    text(out)
        .lines()
        .all(|l| !l.chars().any(tiles_intent::forbidden))
}

#[test]
fn clear_removes_the_cards_and_says_how_many() {
    let dir = std::env::temp_dir().join(format!("tiles-cache-clear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = dir.join("cache");
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    for _ in 0..2 {
        let drawn = tiles(&cache, &["card", intent.to_str().unwrap()]);
        assert!(drawn.status.success(), "{}", text(&drawn));
    }
    let cards = cache.join("cards");
    assert!(
        std::fs::read_dir(&cards).unwrap().count() >= 4,
        "two PNGs and their calls"
    );

    let out = tiles(&cache, &["cache", "clear"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Removed 2 cards.\n");
    assert_eq!(std::fs::read_dir(&cards).unwrap().count(), 0);

    let again = tiles(&cache, &["cache", "clear"]);
    assert_eq!(String::from_utf8_lossy(&again.stdout), "Removed 0 cards.\n");
    let doctor = tiles(&cache, &["doctor"]);
    assert!(doctor.status.success(), "{}", text(&doctor));
    assert!(text(&doctor).contains("(0 saved)"), "{}", text(&doctor));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn anything_but_clear_prints_the_usage_line_and_hostile_text_stays_out() {
    let dir = std::env::temp_dir().join(format!("tiles-cache-usage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = dir.join("cache");
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let entries: Vec<&str> = corpus["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    for args in [
        &["cache"][..],
        &["cache", "empty"],
        &["cache", "clear", "now"],
    ] {
        let out = tiles(&cache, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("tiles: usage: tiles cache clear\n"),
            "{}",
            text(&out)
        );
    }
    // A NUL can be neither an argument nor a path.
    for e in entries.iter().filter(|e| !e.contains('\0')) {
        let out = tiles(&cache, &["cache", e]);
        assert_eq!(out.status.code(), Some(2), "{e:?}");
        assert!(clean(&out), "{e:?}: {:?}", text(&out));
        // A cache named by hostile text that is a file, not a directory,
        // fails with its name in the message, printed clean.
        let named = dir.join(e.replace('/', "_"));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::write(&named, b"");
        let out = tiles(&named, &["cache", "clear"]);
        assert!(clean(&out), "{e:?}: {:?}", text(&out));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
