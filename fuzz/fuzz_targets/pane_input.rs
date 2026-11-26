//! What the terminal sends `tiles pane` (mouse reports, keys, the replies to
//! its probe), through the pane's input parser. No panic, and the events do
//! not depend on how the bytes were split between reads: the first byte
//! picks where the rest is cut in two.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tiles_term::input::Parser;

fuzz_target!(|data: &[u8]| {
    let Some((&cut, bytes)) = data.split_first() else {
        return;
    };
    let whole = Parser::new().parse(bytes);
    let cut = usize::from(cut).min(bytes.len());
    let mut split = Parser::new();
    let mut events = split.parse(&bytes[..cut]);
    events.extend(split.parse(&bytes[cut..]));
    assert_eq!(whole, events);
    let mut one_by_one = Parser::new();
    let mut events = Vec::new();
    for b in bytes {
        one_by_one.feed(std::slice::from_ref(b), &mut events);
    }
    assert_eq!(whole, events);
});
