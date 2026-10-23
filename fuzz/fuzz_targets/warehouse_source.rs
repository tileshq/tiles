//! A warehouse source's SQL as the AI writes it, model references and all:
//! `WarehouseSource::validate`, `references` and `render`. No panic, a
//! refusal's text never carries a character that can drive a terminal or
//! hide text, and rendering leaves no `{{` behind.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tiles_intent::source::{DataError, WarehouseSource};
use tiles_intent::text::is_unsafe_char;

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let Ok(sql) = std::str::from_utf8(rest) else {
        return;
    };
    let target = match first % 4 {
        0 => None,
        1 => Some("dev"),
        2 => Some("mr:4521:full"),
        _ => Some("prod"),
    };
    let source = WarehouseSource {
        sql: sql.to_string(),
        target: target.map(str::to_string),
        limit: None,
    };
    let refusal = source.validate().err().or_else(|| {
        source
            .render(|r| Ok::<_, DataError>(format!("`acme-prod.dbt_marts.{}`", r.name)))
            .map(|rendered| assert!(!rendered.contains("{{"), "{rendered:?}"))
            .err()
    });
    if let Some(e) = refusal {
        let text = e.to_string();
        assert!(!text.chars().any(is_unsafe_char), "{text:?}");
    }
});
