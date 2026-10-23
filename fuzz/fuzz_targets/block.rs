//! Placeholder blocks on a screen row: `block::find` and `block::partial`
//! over any characters, and the block and summary lines Tiles prints for a
//! title. No panic, every token found names a valid card id and row, and no
//! printed line carries a character that can drive a terminal or hide text.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tiles_intent::text::is_unsafe_char;
use tiles_term::{block, sanitize};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let cells: Vec<char> = text.chars().collect();
    for token in block::find(&cells) {
        assert!(block::is_card_id(&token.id), "{token:?}");
        assert!(token.row >= 1 && token.row <= token.rows, "{token:?}");
    }
    if let Some(at) = block::partial(&cells) {
        assert!(at < cells.len());
    }
    let rows = u16::from(data.first().copied().unwrap_or(1) % 40);
    let cols = usize::from(data.get(1).copied().unwrap_or(80));
    for line in block::lines("a1b2c3", rows, &text, cols)
        .iter()
        .chain(&[sanitize::line(&text, 200)])
    {
        assert!(!line.chars().any(is_unsafe_char), "{line:?}");
    }
});
