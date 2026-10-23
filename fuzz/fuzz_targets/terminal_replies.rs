//! The terminal's answers to Tiles' queries (device attributes, XTVERSION,
//! the kitty graphics probe, colours, cell size), as `tiles run` reads them.
//! No panic, and the version text a reply carries is printable safely.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tiles_intent::text::is_unsafe_char;
use tiles_term::detect::Replies;

fuzz_target!(|data: &[u8]| {
    let replies = Replies::parse(data);
    let _ = replies.light_background();
    let shown = format!("{replies:?}");
    assert!(!shown.is_empty());
    if let Some(version) = &replies.xtversion {
        assert!(!version.chars().any(is_unsafe_char), "{version:?}");
    }
});
