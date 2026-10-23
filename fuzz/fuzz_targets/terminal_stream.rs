//! The harness's output through the wrapper's painter, on every route, with
//! resizes and clock ticks between chunks. The painter is compiled from
//! `crates/tiles-cli/src/wrap/` itself, so this fuzzes the shipped code.
//!
//! Input: three bytes choose the route, rows and columns; the rest splits at
//! 0xFF into chunks. A chunk starting with 0xFE and two more bytes is a
//! resize, one starting with 0xFD is a tick of that many tens of
//! milliseconds, and anything else is harness output. No panic may escape,
//! and every chunk of harness output must come out whole and in order, the
//! painter's own bytes between its pieces only where
//! `Painter::feed_painted` puts them ([`passes_through`]).
#![no_main]

use std::rc::Rc;
use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use tiles_term::detect::Route;
use tiles_term::image::Rgba;

/// The wrapper's own modules, read from the binary crate's source.
#[allow(dead_code, unused_imports)]
#[path = "../../crates/tiles-cli/src/wrap"]
mod wrap {
    pub mod escape;
    pub mod paint;
}

use wrap::escape::Tracker;
use wrap::paint::{CardInfo, Painter, Pictures};

/// One card, `a1b2c3`, three rows high, with three entrance frames (frame
/// 0 is never sent, so an entrance needs a visible frame before the settled
/// one); it is also reported fresh and redrawn, so the send-ahead, entrance
/// and swap paths run on every route. Its picture and frames carry a PNG
/// header of the card's size, which the painter reads before it sends an
/// entrance.
#[derive(Default)]
struct Fake {
    calls: u32,
}

impl Pictures for Fake {
    fn card(&mut self, id: &str) -> Option<CardInfo> {
        (id == "a1b2c3").then(|| CardInfo {
            cols: 20,
            rows: 3,
            png: Rc::new(png(b"card")),
            size_px: (40, 18),
            frames: 3,
            frame_ms: 39,
        })
    }
    fn band(&mut self, _: &str, _: u16, count: u16) -> Option<Rgba> {
        Rgba::new(2, u32::from(count) * 6, vec![255; usize::from(count) * 48])
    }
    fn band_png(&mut self, _: &str, first: u16, count: u16) -> Option<Vec<u8>> {
        Some(format!("band {first} {count}").into_bytes())
    }
    fn frames(&mut self, _: &str) -> Vec<Vec<u8>> {
        vec![png(b"frame 0"), png(b"frame 1"), png(b"frame 2")]
    }
    fn frame_band_png(&mut self, _: &str, n: u16, first: u16, count: u16) -> Option<Vec<u8>> {
        Some(format!("frame {n} band {first} {count}").into_bytes())
    }
    fn flat_png(&mut self, w: u32, h: u32, _: (u8, u8, u8)) -> Option<Vec<u8>> {
        Some(format!("flat {w} {h}").into_bytes())
    }
    fn fresh(&mut self) -> Vec<String> {
        self.calls += 1;
        if self.calls == 2 {
            vec!["a1b2c3".into()]
        } else {
            Vec::new()
        }
    }
    fn redrawn(&mut self) -> Vec<String> {
        if self.calls == 5 {
            vec!["a1b2c3".into()]
        } else {
            Vec::new()
        }
    }
}

/// The start and end of a synchronized update: the harness's, before whose
/// end the painter may paint, or the painter's own around its paint.
const SYNC_ON: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";

/// Whether `out`, the painter's output for the harness's `chunk`, passes
/// the harness's bytes through whole and in order, the painter's own bytes
/// only where `Painter::feed_painted` puts them. `tracker` follows the
/// harness's stream across chunks, so the split points are the painter's:
/// a chunk holding the end of an update of the harness's passes up to that
/// end first, the paint going in before the end; after it, and in a chunk
/// without one, the bytes up to the first ground point pass first, then
/// the painter may open its own update, then the bytes up to the last
/// ground point, then the paint, and an unfinished sequence last.
fn passes_through(tracker: &mut Tracker, chunk: &[u8], out: &[u8]) -> bool {
    let (chunk, out) = match chunk.windows(SYNC_END.len()).rposition(|w| w == SYNC_END) {
        Some(at) => {
            let (head, rest) = chunk.split_at(at);
            if !out.starts_with(head) {
                return false;
            }
            tracker.feed(head);
            tracker.feed(SYNC_END);
            let after = &out[head.len()..];
            let Some(end) = after.windows(SYNC_END.len()).position(|w| w == SYNC_END) else {
                return false;
            };
            (&rest[SYNC_END.len()..], &after[end + SYNC_END.len()..])
        }
        None => (chunk, out),
    };
    let mut probe = tracker.clone();
    let mut first = probe.ground().then_some(0);
    let mut last = first;
    for (i, b) in chunk.iter().enumerate() {
        probe.feed(std::slice::from_ref(b));
        if probe.ground() {
            first.get_or_insert(i + 1);
            last = Some(i + 1);
        }
    }
    tracker.feed(chunk);
    let (open, rest) = chunk.split_at(first.unwrap_or(0));
    let (body, tail) = rest.split_at(last.unwrap_or(0) - first.unwrap_or(0));
    let Some(out) = out.strip_prefix(open) else {
        return false;
    };
    let out = out
        .strip_prefix(SYNC_ON)
        .filter(|o| !body.is_empty() && o.starts_with(body))
        .unwrap_or(out);
    let Some(out) = out.strip_prefix(body) else {
        return false;
    };
    out.ends_with(tail)
}

/// A PNG signature and header for a 40 by 18 picture, then `tag`.
fn png(tag: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    out.extend_from_slice(&40u32.to_be_bytes());
    out.extend_from_slice(&18u32.to_be_bytes());
    out.extend_from_slice(tag);
    out
}

fuzz_target!(|data: &[u8]| {
    let [route, rows, cols, rest @ ..] = data else {
        return;
    };
    let route = Route::ALL[usize::from(*route) % Route::ALL.len()];
    let size = |r: u8, c: u8| (1 + u16::from(r % 60), 1 + u16::from(c));
    let (rows, cols) = size(*rows, *cols);
    let mut painter = Painter::new(rows, cols, route, Fake::default());
    if data[0] & 0x80 != 0 {
        painter = painter.with_motion(Some((12, 12, 14)));
    }
    let mut now = Duration::ZERO;
    let mut tracker = Tracker::default();
    for chunk in rest.split(|b| *b == 0xFF) {
        match chunk {
            [0xFE, r, c, ..] => {
                let (rows, cols) = size(*r, *c);
                let _ = painter.resize(rows, cols);
            }
            [0xFD, ms, ..] => {
                now += Duration::from_millis(u64::from(*ms) * 10);
                let _ = painter.tick(now);
            }
            _ => {
                let out = painter.feed(chunk);
                assert!(
                    passes_through(&mut tracker, chunk, &out),
                    "the harness's bytes must pass through whole and in order: {:?} came out as {:?}",
                    String::from_utf8_lossy(chunk),
                    String::from_utf8_lossy(&out)
                );
            }
        }
    }
});
