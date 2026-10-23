//! `tiles render` (ADR-020): a slip it corrects still draws the card, and
//! each correction prints one line to stderr.

use std::path::Path;
use std::process::Command;

#[test]
fn render_prints_each_correction_to_stderr() {
    let dir = std::env::temp_dir().join(format!("tiles-render-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    v["colour"] = "week".into();
    let intent = dir.join("intent.json");
    std::fs::write(&intent, v.to_string()).unwrap();
    let png = dir.join("card.png");
    let out = Command::new(env!("CARGO_BIN_EXE_tiles"))
        .args([
            "render",
            intent.to_str().unwrap(),
            "-o",
            png.to_str().unwrap(),
        ])
        .args(["--theme", "light", "--scale", "0.5"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        stderr,
        "tiles: Tiles left out the keys it does not take: colour.\n"
    );
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    std::fs::remove_dir_all(&dir).unwrap();
}
