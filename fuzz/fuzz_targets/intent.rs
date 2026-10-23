//! A chart intent as a model or an add-on sends it: `parse_request`, then
//! `into_intent` for inline rows. No panic, and a refusal's text never
//! carries a character that can drive a terminal or hide text.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tiles_intent::text::is_unsafe_char;

fuzz_target!(|data: &[u8]| {
    let Ok(json) = std::str::from_utf8(data) else {
        return;
    };
    let refusal = match tiles_intent::parse_request(json) {
        Ok(request) => request.into_intent().err(),
        Err(e) => Some(e),
    };
    if let Some(e) = refusal {
        let text = e.to_string();
        assert!(!text.chars().any(is_unsafe_char), "{text:?}");
    }
});
