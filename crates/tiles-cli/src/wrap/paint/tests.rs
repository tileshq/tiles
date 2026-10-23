//! Byte-level tests of every wrapper route.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Duration;

use tiles_term::detect::Route;
use tiles_term::image::Rgba;
use tiles_term::kitty;

use super::{
    CardInfo, FIRST_IMAGE, LAST_SESSION_IMAGE, Painter, Pictures, SESSION_IMAGES, card_image_id,
};

#[derive(Default)]
struct Fake {
    bands: Vec<(u16, u16)>,
    /// Entrance frame bands cut: frame, first row, rows.
    frame_bands: Vec<(u16, u16, u16)>,
    /// Each frame band is larger than a tick may send, as a dashboard's.
    huge: bool,
    /// Entrance frames the card has.
    frames: u16,
    /// Rows the card covers; 3 when 0.
    rows: u16,
    /// Columns the card covers; 20 when 0.
    cols: u16,
    /// Cards [`Pictures::fresh`] reports next.
    fresh: Vec<String>,
    /// Cards [`Pictures::redrawn`] reports next.
    redrawn: Vec<String>,
    /// The card's picture in place of [`PNG`], once drawn again.
    png: Option<Vec<u8>>,
}

const PNG: &[u8] = b"\x89PNG whole card";

/// A second card the fake has, the same size as the first.
const SECOND: &str = "b0b0b0";

/// The settled card at half resolution, the entrance's last frame: a PNG
/// header 20 pixels wide and 3 a row.
fn half(rows: u16) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(20u32.to_be_bytes());
    png.extend((u32::from(rows) * 3).to_be_bytes());
    png
}

impl Pictures for Fake {
    fn card(&mut self, id: &str) -> Option<CardInfo> {
        let rows = if self.rows == 0 { 3 } else { self.rows };
        (id == "a1b2c3" || id == SECOND).then(|| CardInfo {
            cols: if self.cols == 0 { 20 } else { self.cols },
            rows,
            png: Rc::new(self.png.clone().unwrap_or_else(|| PNG.to_vec())),
            size_px: (40, u32::from(rows) * 6),
            frames: self.frames,
            frame_ms: 50,
        })
    }
    fn band(&mut self, _: &str, first: u16, count: u16) -> Option<Rgba> {
        self.bands.push((first, count));
        Rgba::new(2, u32::from(count) * 6, vec![255; usize::from(count) * 48])
    }
    fn band_png(&mut self, _: &str, first: u16, count: u16) -> Option<Vec<u8>> {
        self.bands.push((first, count));
        Some(format!("band {first} {count}").into_bytes())
    }
    fn frames(&mut self, _: &str) -> Vec<Vec<u8>> {
        let rows = if self.rows == 0 { 3 } else { self.rows };
        (0..self.frames)
            .map(|n| {
                if n + 1 == self.frames {
                    half(rows)
                } else {
                    format!("frame {n}").into_bytes()
                }
            })
            .collect()
    }
    fn frame_band_png(&mut self, _: &str, n: u16, first: u16, count: u16) -> Option<Vec<u8>> {
        self.frame_bands.push((n, first, count));
        if self.huge {
            return Some(vec![b'x'; 70 * 1024]);
        }
        Some(format!("frame {n} band {first} {count}").into_bytes())
    }
    fn flat_png(&mut self, w: u32, h: u32, rgb: (u8, u8, u8)) -> Option<Vec<u8>> {
        Some(format!("flat {w} {h} {rgb:?}").into_bytes())
    }
    fn fresh(&mut self) -> Vec<String> {
        std::mem::take(&mut self.fresh)
    }

    fn redrawn(&mut self) -> Vec<String> {
        std::mem::take(&mut self.redrawn)
    }
}

const BLOCK: &[u8] =
    b"[tile:a1b2c3 01/03] Signups dipped\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";

fn painter(route: Route) -> Painter<Fake> {
    Painter::new(10, 40, route, Fake::default())
}

const SYNC_ON: &[u8] = b"\x1b[?2026h";

/// The paint added to `chunk`: the harness's bytes pass through first,
/// inside the paint's own synchronized update when it opens one, so the
/// paint returned starts with that update's start.
fn added(chunk: &[u8], out: &[u8]) -> Vec<u8> {
    if let Some(rest) = out.strip_prefix(SYNC_ON)
        && rest.starts_with(chunk)
        && !chunk.is_empty()
    {
        return [SYNC_ON, &rest[chunk.len()..]].concat();
    }
    assert!(
        out.starts_with(chunk),
        "the harness's bytes pass through first"
    );
    out[chunk.len()..].to_vec()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn placeholders_upload_once_and_draw_every_row() {
    let mut p = painter(Route::KittyPlaceholders);
    let out = p.feed(BLOCK);
    let paint = added(BLOCK, &out);
    let id = card_image_id("a1b2c3");
    assert!(paint.starts_with(b"\x1b[?2026h"));
    assert!(contains(&paint, &kitty::virtual_png(id, PNG, 20, 3)));
    let rows = kitty::placeholder_rows(id, 20, 3);
    for (y, row) in rows.iter().enumerate() {
        let expected = [
            format!("\x1b[{};1H\x1b[0m", y + 1).into_bytes(),
            row.as_bytes().to_vec(),
        ]
        .concat();
        assert!(contains(&paint, &expected), "row {y}");
    }
    // The cursor goes back where the harness left it: row 4, column 1.
    assert!(paint.ends_with(b"\x1b[4;1H\x1b[?2026l"));
    // Unrelated output repaints nothing and uploads nothing again.
    let out = p.feed(b"more text");
    assert_eq!(out, b"more text");
}

#[test]
fn a_half_typed_block_shows_no_card_and_hides_its_tokens() {
    let mut p = painter(Route::KittyPlaceholders);
    let chunk = b"[tile:a1b2c3 01/03] Signups\r\n[tile:a1b2c3 02/03]\r\n[tile:a1";
    let paint = added(chunk, &p.feed(chunk));
    assert!(
        !contains(&paint, b"\x1b_G"),
        "no picture before the block is whole"
    );
    assert!(contains(
        &paint,
        &[b"\x1b[1;1H\x1b[0m".as_slice(), &[b' '; 19]].concat()
    ));
    assert!(
        contains(
            &paint,
            &[b"\x1b[3;1H\x1b[0m".as_slice(), &[b' '; 8]].concat()
        ),
        "the partial token"
    );
    let rest = b"b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    assert!(contains(&paint, b"\x1b_Ga=T,f=100,t=d,q=2,U=1"));
}

#[test]
fn nothing_is_painted_inside_an_escape_sequence() {
    let mut p = painter(Route::KittyPlaceholders);
    // The paint goes in at the last ground point, before the sequence the
    // read ends inside, and nothing goes in until that sequence ends.
    let chunk = [BLOCK, b"\x1b[3".as_slice()].concat();
    let out = p.feed(&chunk);
    assert!(out.ends_with(b"\x1b[3") && contains(&out, b"\x1b_G"));
    assert_eq!(p.feed(b"1"), b"1", "still inside it");
    assert_eq!(p.feed(b"m"), b"m", "nothing new to paint");
}

#[test]
fn the_harness_sync_frame_is_not_split() {
    let mut p = painter(Route::KittyPlaceholders);
    // Half a frame: nothing is painted from it.
    let chunk = [b"\x1b[?2026h".as_slice(), BLOCK].concat();
    assert_eq!(p.feed(&chunk), chunk, "no paint inside a half-drawn frame");
    // The frame's end: the paint goes in just before it, inside the update.
    let out = p.feed(b"\x1b[?2026l");
    assert!(
        out.ends_with(b"\x1b[?2026l"),
        "{}",
        String::from_utf8_lossy(&out)
    );
    let paint = &out[..out.len() - 8];
    assert!(
        contains(paint, "\u{10EEEE}".as_bytes()),
        "the card is drawn"
    );
    assert!(
        !contains(paint, b"\x1b[?2026"),
        "inside the harness's own update"
    );
}

#[test]
fn a_held_synchronized_update_is_painted_after_a_second() {
    let mut p = painter(Route::KittyPlaceholders);
    let chunk = [b"\x1b[?2026h".as_slice(), BLOCK].concat();
    assert_eq!(p.feed(&chunk), chunk);
    p.set_now(Duration::from_millis(1100));
    let out = p.feed(b" ");
    assert!(contains(&out, "\u{10EEEE}".as_bytes()));
}

#[test]
fn precut_places_the_whole_card_then_a_cut_under_a_new_id() {
    let mut p = Painter::new(4, 40, Route::KittyPrecut, Fake::default());
    let chunk = [b"\r\n".as_slice(), BLOCK].concat();
    let paint = added(&chunk, &p.feed(&chunk));
    assert!(contains(
        &paint,
        &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 3, None, 1)
    ));
    // One more line scrolls the first card row off the top.
    let paint = added(b"x", &p.feed(b"x"));
    assert!(paint.is_empty(), "same place, nothing to redo: {paint:?}");
    let paint = added(b"\r\n", &p.feed(b"\r\n"));
    assert_eq!(p.pictures.bands, [(1, 2)]);
    assert!(
        contains(
            &paint,
            &kitty::show_png_over(FIRST_IMAGE + 2, b"band 1 2", 20, 2, None, 2)
        ),
        "over the old one"
    );
    assert!(
        !contains(&paint, &kitty::delete(FIRST_IMAGE)),
        "the old picture stays while the new one shows"
    );
    // A tick later the old one goes.
    let paint = p.tick(Duration::from_millis(50));
    assert_eq!(paint, kitty::delete(FIRST_IMAGE));
    // Clearing the screen removes the card.
    let paint = added(b"\x1b[2J", &p.feed(b"\x1b[2J"));
    assert!(contains(&paint, &kitty::delete(FIRST_IMAGE + 2)));
}

#[test]
fn sixel_draws_a_picture_and_never_reaches_the_last_row() {
    let mut p = Painter::new(4, 40, Route::Sixel, Fake::default());
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(contains(&paint, b"\x1bP0;1;0q"));
    assert_eq!(p.pictures.bands, [(0, 3)]);
    assert!(
        !contains(&paint, b"\x1b_G"),
        "no kitty commands on the sixel route"
    );
}

#[test]
fn only_one_copy_of_a_card_is_drawn() {
    let mut p = Painter::new(12, 40, Route::KittyPrecut, Fake::default());
    let chunk = [BLOCK, b"\r\n".as_slice(), BLOCK].concat();
    let paint = added(&chunk, &p.feed(&chunk));
    assert_eq!(paint.windows(5).filter(|w| *w == b"\x1b_Ga=").count(), 1);
}

#[test]
fn the_alternate_screen_drops_placements() {
    let mut p = Painter::new(6, 40, Route::KittyPrecut, Fake::default());
    p.feed(BLOCK);
    let paint = added(b"\x1b[?1049h", &p.feed(b"\x1b[?1049h"));
    assert!(contains(&paint, &kitty::delete(FIRST_IMAGE)));
}

#[test]
fn the_paint_writes_only_its_own_sequences() {
    let hostile = "[tile:a1b2c3 01/03] \x1b]52;c;ZXZpbA==\x07 \u{202e}evil\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
    for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
        let mut p = painter(route);
        let paint = added(hostile.as_bytes(), &p.feed(hostile.as_bytes()));
        assert!(
            !contains(&paint, b"\x1b]"),
            "{route:?}: no OSC from the painter"
        );
        assert!(!contains(&paint, "\u{202e}".as_bytes()), "{route:?}");
    }
}

#[test]
fn an_unknown_card_is_left_as_text() {
    let mut p = painter(Route::KittyPlaceholders);
    let chunk = b"[tile:ffffff 01/01] other machine\r\n";
    let paint = added(chunk, &p.feed(chunk));
    assert!(!contains(&paint, b"\x1b_G"));
}

#[test]
fn a_copy_with_more_rows_than_its_card_is_not_drawn() {
    let chunk = b"[tile:a1b2c3 01/04] t\r\n[tile:a1b2c3 02/04]\r\n[tile:a1b2c3 03/04]\r\n[tile:a1b2c3 04/04]\r\n";
    for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
        let mut p = painter(route);
        let paint = added(chunk, &p.feed(chunk));
        assert!(
            !contains(&paint, b"\x1b_G") && !contains(&paint, b"\x1bP"),
            "{route:?}"
        );
    }
}

// A model that puts the block inside a code fence: Pi shows both
// fence lines and indents the block two columns.

const FENCED: &[u8] = b"Here it is:\r\n```tiles\r\n  [tile:a1b2c3 01/03] Signups dipped\r\n  [tile:a1b2c3 02/03]\r\n  [tile:a1b2c3 03/03]\r\n```\r\nW3 dipped.\r\n";

/// Each row of the user's terminal after `chunk` goes through a fresh
/// painter on `route`, trimmed at its end.
fn shown(route: Route, chunk: &[u8]) -> Vec<String> {
    let mut p = painter(route);
    let mut term = vt100::Parser::new(10, 40, 0);
    term.process(&p.feed(chunk));
    term.screen()
        .rows(0, 40)
        .map(|r| r.trim_end().to_string())
        .collect()
}

#[test]
fn a_fence_right_above_and_below_a_block_is_blanked() {
    for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
        let rows = shown(route, FENCED);
        assert_eq!(rows[0], "Here it is:", "{route:?}");
        assert_eq!(rows[1], "", "{route:?}: the opening fence");
        assert_eq!(rows[5], "", "{route:?}: the closing fence");
        assert_eq!(rows[6], "W3 dipped.", "{route:?}");
    }
    // Any fence of three or more backticks, with or without its word.
    let chunk = b"````text\r\n[tile:a1b2c3 01/03] t\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n````  \r\n";
    let rows = shown(Route::KittyPlaceholders, chunk);
    assert_eq!((rows[0].as_str(), rows[4].as_str()), ("", ""));
}

#[test]
fn a_fence_or_text_not_right_beside_a_block_stays() {
    // A blank row between the fence and the block.
    let chunk = b"```tiles\r\n\r\n[tile:a1b2c3 01/03] t\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n\r\n```\r\n";
    let rows = shown(Route::KittyPlaceholders, chunk);
    assert_eq!((rows[0].as_str(), rows[6].as_str()), ("```tiles", "```"));
    // On the adjacent rows, but more than a fence, or right of the block.
    for (above, below) in [
        ("``` tiles now", "Use ```tiles"),
        ("``", "``x`"),
        ("  ```", "say ```"),
    ] {
        let chunk = format!(
            "{above}\r\n[tile:a1b2c3 01/03] t\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n{below}\r\n"
        );
        let rows = shown(Route::KittyPlaceholders, chunk.as_bytes());
        assert_eq!((rows[0].as_str(), rows[4].as_str()), (above, below));
    }
}

#[test]
fn a_block_with_no_fence_leaves_the_rows_around_it_alone() {
    let chunk = [b"Here it is:\r\n".as_slice(), BLOCK, b"W3 dipped.\r\n"].concat();
    for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
        let mut p = painter(route);
        let paint = added(&chunk, &p.feed(&chunk));
        assert!(
            !contains(&paint, b"\x1b[1;") && !contains(&paint, b"\x1b[5;"),
            "{route:?}: nothing painted on the text rows"
        );
        let rows = shown(route, &chunk);
        assert_eq!(
            (rows[0].as_str(), rows[4].as_str()),
            ("Here it is:", "W3 dipped.")
        );
    }
}

#[test]
fn a_fence_the_block_left_gets_its_text_back() {
    let mut p = painter(Route::KittyPlaceholders);
    let mut term = vt100::Parser::new(10, 40, 0);
    term.process(&p.feed(FENCED));
    assert_eq!(term.screen().contents_between(1, 0, 1, 40).trim(), "");
    // The harness redraws the block's rows as other text.
    let redraw = b"\x1b[3;1H\x1b[2Kone\x1b[4;1H\x1b[2Ktwo\x1b[5;1H\x1b[2Kthree\x1b[8;1H";
    term.process(&p.feed(redraw));
    assert_eq!(
        term.screen().contents_between(1, 0, 1, 40).trim(),
        "```tiles"
    );
    assert_eq!(term.screen().contents_between(5, 0, 5, 40).trim(), "```");
}

#[test]
fn a_fence_row_from_the_hostile_corpus_adds_no_sequence_of_its_own() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut written_back = 0;
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        let chunk = format!(
            "```{e}\r\n[tile:a1b2c3 01/03] t\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n{e}```\r\n"
        );
        // The block goes, so a fence blanked is written back.
        let gone = b"\x1b[2;1H\x1b[2Kone\x1b[3;1H\x1b[2Ktwo\x1b[4;1H\x1b[2Kthree\x1b[8;1H";
        for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
            let mut p = painter(route);
            let out = p.feed(chunk.as_bytes());
            let paint = String::from_utf8_lossy(&added(chunk.as_bytes(), &out)).into_owned();
            assert!(
                !paint.contains("pwned") && !paint.contains("evil"),
                "{route:?} {e:?}: a blank is spaces"
            );
            // A write-back is the harness's own text, without its sequences.
            let back = String::from_utf8_lossy(&added(gone, &p.feed(gone))).into_owned();
            if back.contains("\u{1b}[1;1H\u{1b}[0m") {
                written_back += 1;
            }
            for paint in [paint, back] {
                assert!(
                    !paint.contains("\u{1b}]")
                        && !paint.contains('\u{7}')
                        && !paint.contains('\u{9b}'),
                    "{route:?} {e:?}"
                );
            }
        }
    }
    assert!(
        written_back > 10,
        "{written_back} fences reached the write-back"
    );
}

#[test]
fn a_harness_line_under_a_card_from_the_hostile_corpus_adds_no_sequence_of_its_own() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut written_back = 0;
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        // The block's first row, and the harness's own line under it, which
        // the card covers until the block leaves.
        let chunk = format!("[tile:a1b2c3 01/03] t\r\n{e}");
        let mut p = painter(Route::KittyPrecut);
        let out = p.feed(chunk.as_bytes());
        let paint = String::from_utf8_lossy(&added(chunk.as_bytes(), &out)).into_owned();
        assert!(
            !paint.contains("pwned") && !paint.contains("evil"),
            "{e:?}: a blank is spaces"
        );
        let gone = b"\x1b[1;1H\x1b[2K";
        let back = String::from_utf8_lossy(&added(gone, &p.feed(gone))).into_owned();
        if back.contains("\u{1b}[2;1H\u{1b}[0m") {
            written_back += 1;
        }
        for paint in [paint, back] {
            assert!(
                !paint.contains("\u{1b}]") && !paint.contains('\u{7}') && !paint.contains('\u{9b}'),
                "{e:?}"
            );
        }
    }
    assert!(
        written_back > 10,
        "{written_back} lines reached the write-back"
    );
}

#[test]
fn the_hostile_corpus_in_harness_output_adds_no_sequence_of_its_own() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        let chunk =
            format!("[tile:a1b2c3 01/03] {e}\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n");
        for route in [Route::KittyPlaceholders, Route::KittyPrecut, Route::Sixel] {
            let mut p = painter(route);
            let out = p.feed(chunk.as_bytes());
            let paint = String::from_utf8_lossy(&out[chunk.len()..]).into_owned();
            // The painter writes only CUP, SGR, synchronized output and its
            // own graphics commands; the corpus's own text never reappears.
            assert!(
                !paint.contains("\u{1b}]") && !paint.contains('\u{7}') && !paint.contains('\u{9b}'),
                "{route:?} {e:?}"
            );
            assert!(
                !paint.contains("pwned") && !paint.contains("evil"),
                "{route:?} {e:?}"
            );
        }
    }
}

// vt100 0.16.2 panics on these (screen.rs:870, grid.rs:683 and 689); the
// painter must contain each panic and keep passing the harness's bytes.

#[test]
fn a_wide_character_at_the_new_edge_after_a_resize_does_not_panic() {
    let mut p = Painter::new(24, 80, Route::KittyPrecut, Fake::default());
    let first = format!("{}✅", "x".repeat(58));
    assert_eq!(p.feed(first.as_bytes()), first.as_bytes());
    let _ = p.resize(24, 59);
    assert_eq!(p.feed(b"K"), b"K");
    assert_eq!(p.feed(BLOCK), BLOCK, "only pass-through once broken");
}

#[test]
fn a_cjk_character_at_the_new_edge_after_a_resize_does_not_panic() {
    let mut p = Painter::new(24, 47, Route::KittyPrecut, Fake::default());
    let _ = p.feed(format!("{}本語", "x".repeat(28)).as_bytes());
    let _ = p.resize(24, 31);
    assert_eq!(p.feed(b"K"), b"K");
}

#[test]
fn a_one_row_screen_that_wraps_does_not_panic() {
    let mut p = Painter::new(1, 80, Route::KittyPrecut, Fake::default());
    let chunk = [b'x'; 81];
    assert_eq!(p.feed(&chunk), chunk);
}

#[test]
fn a_broken_painter_removes_its_pictures_and_draws_nothing_more() {
    let mut p = Painter::new(6, 80, Route::KittyPrecut, Fake::default());
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(contains(&paint, b"\x1b_Ga=T"));
    let _ = p.feed(format!("{}✅", "x".repeat(58)).as_bytes());
    let out = p.resize(6, 59);
    let out = [out, p.feed(b"K")].concat();
    assert!(p.broken());
    assert!(contains(&out, &kitty::delete(FIRST_IMAGE)));
    assert_eq!(p.resize(6, 40), Vec::<u8>::new());
    assert_eq!(p.feed(BLOCK), BLOCK);
}

#[test]
fn a_broken_painter_deletes_its_pictures_only_at_ground() {
    let mut p = Painter::new(10, 80, Route::KittyPrecut, Fake::default());
    let _ = p.feed(format!("{}✅", "x".repeat(58)).as_bytes());
    let _ = p.resize(10, 59);
    let chunk = [b"\x1b[4;1H".as_slice(), BLOCK].concat();
    let paint = added(&chunk, &p.feed(&chunk));
    assert!(contains(&paint, b"\x1b_Ga=T"));
    let images: Vec<u32> = p.strips.values().flatten().map(|s| s.image).collect();
    assert_eq!(images.len(), 1);
    // The panic comes with a read that ends inside the harness's own CSI.
    assert_eq!(p.feed(b"\x1b[1;59HK\x1b[3"), b"\x1b[1;59HK\x1b[3");
    assert!(p.broken());
    assert_eq!(p.feed(b"1"), b"1");
    assert_eq!(p.feed(b"mok\xe6\x9c"), b"mok\xe6\x9c");
    assert_eq!(
        p.feed(b"\xac"),
        [b"\xac".as_slice(), kitty::delete(images[0]).as_slice()].concat()
    );
    assert_eq!(p.feed(BLOCK), BLOCK);
}

// Found by the painter's TLA+ model (verify/formal/painter, GaveUpCleanly):
// giving up removed the early placement under the settled card's image id,
// so an entrance placed early stayed on screen over the harness's text.
#[test]
fn a_broken_painter_removes_an_entrance_placed_early() {
    let fake = Fake {
        frames: 3,
        fresh: vec!["a1b2c3".into()],
        ..Fake::default()
    };
    let mut p = Painter::new(1, 80, Route::KittyPlaceholders, fake).with_motion(Some(GREY));
    let _ = p.tick(Duration::from_millis(50));
    let anim = FIRST_IMAGE;
    let first = b"[tile:a1b2c3 01/03] Signups";
    let paint = added(first, &p.feed(first));
    let early = format!("\x1b_Ga=p,q=2,C=1,i={anim},p=2,");
    assert!(
        contains(&paint, early.as_bytes()),
        "placed early with its entrance"
    );
    let out = p.feed(&[b'x'; 81]);
    assert!(p.broken());
    let removed = [
        kitty::delete_placement(anim, 2),
        kitty::delete(FIRST_IMAGE + 2),
        kitty::delete(anim),
    ]
    .concat();
    assert!(
        contains(&out, &removed),
        "{}",
        String::from_utf8_lossy(&out)
    );
}

/// A small deterministic generator, so a failure names its seed.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
}

const PIECES: &[&[u8]] = &[
    b"x",
    b"hello world ",
    b"\r\n",
    b"\n",
    b"\r",
    b"\t",
    b"\x08",
    "✅".as_bytes(),
    "本語".as_bytes(),
    "👍🏽".as_bytes(),
    "e\u{301}".as_bytes(),
    b"\xe6\x9c",
    b"\xac",
    b"\xff",
    b"[tile:a1b2c3 01/03] Signups\r\n",
    b"[tile:a1b2c3 02/03]\r\n",
    b"[tile:a1b2c3 03/03]\r\n",
    b"[tile:a1b2",
    b"\x1b[",
    b"\x1b[2J",
    b"\x1b[H",
    b"\x1b[5;70H",
    b"\x1b[999;999H",
    b"\x1b[K",
    b"\x1b[3L",
    b"\x1b[2M",
    b"\x1b[4@",
    b"\x1b[3P",
    b"\x1b[2;3r",
    b"\x1b[r",
    b"\x1b[S",
    b"\x1b[T",
    b"\x1b[?1049h",
    b"\x1b[?1049l",
    b"\x1b[?2026h",
    b"\x1b[?2026l",
    b"\x1b[?7l",
    b"\x1b[?7h",
    b"\x1b[31;1m",
    b"\x1b]0;title\x07",
    b"\x1b]52;c;ZXZpbA==\x1b\\",
    b"\x1bP1$q",
    b"\x1b\\",
    b"\x07",
    b"\x90",
    b"\x9b",
    b"\x9d",
    b"\x1bM",
    b"\x1bD",
    b"\x1b7",
    b"\x1b8",
    b"\x1bc",
];

#[test]
fn random_output_and_resizes_never_panic_and_always_pass_through() {
    for seed in 1..=300u64 {
        let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let size = |rng: &mut XorShift| {
            let rows = [1, 1, 2, 3, 24, 50][rng.below(6)];
            let cols = [1, 1, 2, 3, 31, 59, 80, 200][rng.below(8)];
            (rows, cols)
        };
        let routes = [
            Route::KittyPlaceholders,
            Route::KittyPrecut,
            Route::Sixel,
            Route::Text,
        ];
        let (rows, cols) = size(&mut rng);
        let mut p = Painter::new(rows, cols, routes[rng.below(4)], Fake::default());
        for _ in 0..80 {
            if rng.below(8) == 0 {
                let (rows, cols) = size(&mut rng);
                let _ = p.resize(rows, cols);
                continue;
            }
            let chunk: Vec<u8> = (0..=rng.below(6))
                .flat_map(|_| PIECES[rng.below(PIECES.len())].iter().copied())
                .collect();
            let owed = p.pending.clone();
            let was_broken = p.broken();
            let out = p.feed(&chunk);
            // The harness's bytes pass through whole and in order; the paint
            // goes in between them, at ground.
            let mut extra = Vec::new();
            let mut next = 0;
            for b in &out {
                if next < chunk.len() && *b == chunk[next] {
                    next += 1;
                } else {
                    extra.push(*b);
                }
            }
            assert_eq!(next, chunk.len(), "seed {seed}: bytes reordered or lost");
            let extra = extra.as_slice();
            if p.broken() && !extra.is_empty() {
                assert!(p.tracker.ground(), "seed {seed}: deletes only at ground");
                assert!(
                    !was_broken || extra == owed,
                    "seed {seed}: once broken only owed deletes are added"
                );
            }
        }
    }
}

#[test]
fn blanks_and_card_rows_sit_on_the_background_beside_the_block() {
    let mut p = painter(Route::KittyPlaceholders);
    let on = "\x1b[48;2;10;10;10m  ";
    let chunk =
        format!("{on}[tile:a1b2c3 01/03] Signups\r\n{on}[tile:a1b2c3 02/03]\r\n{on}[tile:a1")
            .into_bytes();
    let paint = added(&chunk, &p.feed(&chunk));
    let sgr = b"\x1b[0;48;2;10;10;10m";
    assert!(
        contains(
            &paint,
            &[b"\x1b[1;3H".as_slice(), sgr, &[b' '; 19]].concat()
        ),
        "a token waiting for its block"
    );
    assert!(
        contains(&paint, &[b"\x1b[3;3H".as_slice(), sgr, &[b' '; 8]].concat()),
        "a half-typed token"
    );
    let rest = b"b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    let rows = kitty::placeholder_rows(card_image_id("a1b2c3"), 20, 3);
    for (y, row) in rows.iter().enumerate() {
        let expected = [format!("\x1b[{};3H", y + 1).as_bytes(), sgr, row.as_bytes()].concat();
        assert!(contains(&paint, &expected), "row {y}");
    }
    assert!(
        !contains(&paint, b"\x1b[0m\x1b[38;2;"),
        "no default-background card cell"
    );
}

// The entrance and the early placement (kitty).

const GREY: (u8, u8, u8) = (30, 30, 30);

fn animated(rows: u16) -> Painter<Fake> {
    let fake = Fake {
        frames: 3,
        rows,
        ..Fake::default()
    };
    Painter::new(10, 40, Route::KittyPlaceholders, fake).with_motion(Some(GREY))
}

/// [`animated`], with the card made and its entrance sent ahead, as when
/// the MCP server writes it before the harness types its block: the
/// entrance is image [`FIRST_IMAGE`], its backing the next.
fn sent_ahead(rows: u16) -> Painter<Fake> {
    let mut p = animated(rows);
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    p
}

fn crop(h: u32) -> kitty::Crop {
    kitty::Crop {
        x: 0,
        y: 0,
        w: 40,
        h,
    }
}

/// [`crop`] in the entrance's half-resolution pixels.
fn half_crop(h: u32) -> kitty::Crop {
    kitty::Crop {
        x: 0,
        y: 0,
        w: 20,
        h,
    }
}

/// The entrance as the painter sends it, an image of its own: the settled
/// card at half resolution, the one visible entrance frame 40 ms on (frame
/// 0, the card at no opacity, is never sent), and the settled card again.
fn animation(rows: u16) -> Vec<u8> {
    let image = FIRST_IMAGE;
    [
        kitty::transmit_png(image, &half(rows)),
        kitty::frame_png(image, b"frame 1", 40),
        kitty::frame_png(image, &half(rows), 40),
    ]
    .concat()
}

#[test]
fn a_new_card_is_sent_settled_then_its_entrance_follows() {
    let mut p = animated(3);
    let id = card_image_id("a1b2c3");
    p.pictures.fresh = vec!["a1b2c3".into()];
    let ahead = p.tick(Duration::from_millis(50));
    assert_eq!(
        ahead,
        [kitty::virtual_png(id, PNG, 20, 3), animation(3)].concat(),
        "the crisp card first, so it can show at once"
    );
    assert!(p.tick(Duration::from_millis(100)).is_empty(), "sent once");
}

#[test]
fn in_kitty_the_whole_card_shows_on_its_first_row_and_plays_its_entrance() {
    let mut p = sent_ahead(3);
    let id = card_image_id("a1b2c3");
    let anim = FIRST_IMAGE;
    let first = b"[tile:a1b2c3 01/03] Signups\r\n";
    let paint = added(first, &p.feed(first));
    assert!(
        !contains(&paint, &kitty::transmit_png(anim, &half(3))) && !contains(&paint, b"a=f,"),
        "nothing sent again"
    );
    assert!(
        contains(&paint, &kitty::show_frame(anim, 2)),
        "shows the first visible entrance frame, kitty's second"
    );
    // The backing, the entrance's size in the conversation's colour, then
    // the entrance over all three rows the block will take.
    let mask = FIRST_IMAGE + 2;
    assert!(contains(
        &paint,
        &kitty::transmit_png(mask, b"flat 20 9 (30, 30, 30)")
    ));
    let placed = [
        b"\x1b[1;1H".to_vec(),
        kitty::place(mask, 1, 20, 3, 1, Some(half_crop(9))),
        kitty::place(anim, 2, 20, 3, 2, Some(half_crop(9))),
    ]
    .concat();
    assert!(
        contains(&paint, &placed),
        "{}",
        String::from_utf8_lossy(&paint)
    );
    assert!(
        contains(
            &paint,
            &[b"\x1b[1;1H\x1b[0m".as_slice(), &[b' '; 19]].concat()
        ),
        "the typed token is blanked under it"
    );
    // The rest of the block: its rows draw the crisp card, nothing is sent
    // or placed again, and the entrance stays over it while it plays.
    let rest = b"[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    assert!(contains(
        &paint,
        kitty::placeholder_rows(id, 20, 3)[2].as_bytes()
    ));
    assert!(!contains(&paint, b"a=t") && !contains(&paint, b"a=p,"));
    assert!(!contains(&paint, &kitty::delete_placement(anim, 2)));
    // Over: stopped on its settled frame (3 of 3), then the entrance and
    // its backing are deleted, leaving the crisp card in the text.
    let end = p.tick(Duration::from_secs(2));
    let handed = [
        kitty::show_frame(anim, 3),
        kitty::delete_placement(anim, 2),
        kitty::delete(mask),
        kitty::delete(anim),
    ]
    .concat();
    assert_eq!(end, handed, "{}", String::from_utf8_lossy(&end));
    assert!(p.tick(Duration::from_secs(3)).is_empty(), "nothing more");
}

#[test]
fn the_entrance_shows_the_frame_for_the_time_passed_then_stops() {
    let mut p = sent_ahead(3);
    let anim = FIRST_IMAGE;
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(
        contains(&paint, &kitty::show_frame(anim, 2)),
        "a block typed whole plays too, from its first visible frame"
    );
    // Shown at 50 ms as if begun at 0 ms: frame 2 until 100 ms, then the
    // settled card, frame 3, with no frame shown twice.
    assert!(
        p.tick(Duration::from_millis(80)).is_empty(),
        "still frame 2"
    );
    assert_eq!(
        p.tick(Duration::from_millis(100)),
        [
            kitty::show_frame(anim, 3),
            kitty::delete_placement(anim, 2),
            kitty::delete(anim + 2),
            kitty::delete(anim),
        ]
        .concat(),
        "stopped, then handed over to the crisp card"
    );
    assert!(p.tick(Duration::from_millis(150)).is_empty(), "once");
}

#[test]
fn a_late_tick_skips_to_the_frame_for_the_time_passed() {
    let fake = Fake {
        frames: 6,
        ..Fake::default()
    };
    let mut p = Painter::new(10, 40, Route::KittyPlaceholders, fake).with_motion(Some(GREY));
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    let anim = FIRST_IMAGE;
    let _ = p.feed(BLOCK);
    // Four visible entrance frames 50 ms apart, the first (kitty's frame 2)
    // shown at 50 ms; the settled card is frame 6, at 250 ms.
    assert_eq!(
        p.tick(Duration::from_millis(100)),
        kitty::show_frame(anim, 3)
    );
    assert_eq!(
        p.tick(Duration::from_millis(210)),
        kitty::show_frame(anim, 5),
        "frame 4 is skipped, not shown late"
    );
    assert!(contains(
        &p.tick(Duration::from_millis(250)),
        &kitty::show_frame(anim, 6)
    ));
}

#[test]
fn an_entrance_over_while_the_card_is_still_placed_early_keeps_its_frames_until_handed_over() {
    let mut p = sent_ahead(3);
    let anim = FIRST_IMAGE;
    let first = b"[tile:a1b2c3 01/03] Signups\r\n";
    let _ = p.feed(first);
    let end = p.tick(Duration::from_secs(2));
    assert!(contains(&end, &kitty::show_frame(anim, 3)));
    assert!(
        !contains(&end, &kitty::delete(anim)),
        "the early placement still shows the image"
    );
    let rest = b"[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    assert!(contains(
        &paint,
        &[
            kitty::delete_placement(anim, 2),
            kitty::delete(anim + 2),
            kitty::delete(anim)
        ]
        .concat()
    ));
}

#[test]
fn the_run_waits_for_the_harness_to_end_its_synchronized_update() {
    let mut p = sent_ahead(3);
    let anim = FIRST_IMAGE;
    // The card goes in at the end of the harness's update, on its first
    // entrance frame; its clock starts once the update has ended.
    let chunk = [b"\x1b[?2026h".as_slice(), BLOCK].concat();
    assert_eq!(p.feed(&chunk), chunk);
    // Painted at the update's end, 350 ms on: the card shows its first
    // entrance frame, and no frame moved on while the update was held.
    assert!(!contains(&p.tick(Duration::from_millis(400)), b"a=a,"));
    let out = p.feed(b"\x1b[?2026l");
    let end = out.windows(8).position(|w| w == b"\x1b[?2026l").unwrap();
    assert!(contains(&out[..end], &kitty::show_frame(anim, 2)));
    // The clock starts at the update's end, 400 ms, one frame in.
    assert!(!contains(&p.tick(Duration::from_millis(420)), b"a=a,"));
    assert!(contains(
        &p.tick(Duration::from_millis(450)),
        &kitty::show_frame(anim, 3)
    ));
}

#[test]
fn a_block_typed_before_its_entrance_arrives_shows_the_crisp_card_settled() {
    let mut p = animated(3);
    let id = card_image_id("a1b2c3");
    let first = b"[tile:a1b2c3 01/03] Signups\r\n";
    let paint = added(first, &p.feed(first));
    assert!(contains(&paint, &kitty::virtual_png(id, PNG, 20, 3)));
    assert!(contains(
        &paint,
        &kitty::place(id, 2, 20, 3, 2, Some(crop(18)))
    ));
    assert!(!contains(&paint, b"a=a,"), "no entrance to run");
    // The queued entrance is dropped, never sent.
    assert!(!contains(&p.tick(Duration::from_millis(100)), b"a=f,"));
    let rest = b"[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    assert!(contains(&paint, &kitty::delete_placement(id, 2)));
}

#[test]
fn a_card_stops_short_of_text_the_harness_writes_beside_its_block() {
    let mut p = painter(Route::KittyPlaceholders);
    let id = card_image_id("a1b2c3");
    // A side panel's line at column 19, right of the block's second row,
    // inside the card's 20 columns.
    let block = "[tile:a1b2c3 01/03] T\r\n[tile:a1b2c3 02/03]\x1b[2;20H\u{2502}Context\r\n[tile:a1b2c3 03/03]\r\n";
    let paint = added(block.as_bytes(), &p.feed(block.as_bytes()));
    let cut = kitty::placeholder_rows(id, 19, 3);
    let whole = kitty::placeholder_rows(id, 20, 3);
    for (y, row) in cut.iter().enumerate() {
        assert!(contains(&paint, row.as_bytes()), "row {y} cut at the panel");
        assert!(!contains(&paint, whole[y].as_bytes()), "row {y} not whole");
    }
}

#[test]
fn a_card_stops_at_the_harness_side_panel() {
    let mut p = Painter::new(10, 140, Route::KittyPlaceholders, Fake::default())
        .with_panel(Some((120, 130)));
    let id = card_image_id("a1b2c3");
    let paint = added(BLOCK, &p.feed(BLOCK));
    // A 140-column terminal keeps 10 columns left of a 130-column panel.
    assert!(contains(
        &paint,
        kitty::placeholder_rows(id, 10, 3)[1].as_bytes()
    ));
    assert!(!contains(
        &paint,
        kitty::placeholder_rows(id, 20, 3)[1].as_bytes()
    ));
}

#[test]
fn a_cut_picture_blanks_only_the_columns_it_covers() {
    for route in [Route::KittyPrecut, Route::Sixel] {
        let mut p = Painter::new(10, 140, route, Fake::default()).with_panel(Some((120, 130)));
        let paint = added(BLOCK, &p.feed(BLOCK));
        assert!(contains(&paint, &[b' '; 10]), "{route:?}");
        assert!(
            !contains(&paint, &[b' '; 11]),
            "{route:?}: the panel's cells stay"
        );
    }
}

#[test]
fn the_early_card_stops_at_the_harness_footer() {
    let mut p = animated(6);
    let id = card_image_id("a1b2c3");
    // Row 1 typed, then the harness's own input box four rows down.
    let chunk = "[tile:a1b2c3 01/06] T\r\n\r\n\r\n\r\n\u{256d}\u{2500}\u{2500}\u{256e}\r\n";
    let paint = added(chunk.as_bytes(), &p.feed(chunk.as_bytes()));
    assert!(contains(
        &paint,
        &kitty::place(id, 2, 20, 4, 2, Some(crop(24)))
    ));
}

#[test]
fn a_folded_copy_is_not_placed_early() {
    let mut p = animated(3);
    let chunk = "[tile:a1b2c3 01/03] T\r\n  \u{2026} +2 lines\r\n";
    let paint = added(chunk.as_bytes(), &p.feed(chunk.as_bytes()));
    assert!(
        !contains(&paint, b"a=p,"),
        "{}",
        String::from_utf8_lossy(&paint)
    );
}

#[test]
fn an_early_card_whose_block_leaves_the_screen_is_removed() {
    let mut p = animated(3);
    let id = card_image_id("a1b2c3");
    let first = b"[tile:a1b2c3 01/03] Signups\r\n";
    let _ = p.feed(first);
    let clear = b"\x1b[2J\x1b[H";
    let paint = added(clear, &p.feed(clear));
    assert!(contains(&paint, &kitty::delete_placement(id, 2)));
    // The backing: the card's queued entrance took the first id.
    assert!(contains(&paint, &kitty::delete(FIRST_IMAGE + 2)));
}

#[test]
fn the_backing_takes_the_colour_beside_the_block() {
    let mut p = animated(3);
    let on = "\x1b[48;2;10;10;10m  ";
    let chunk = format!("{on}[tile:a1b2c3 01/03] Signups\r\n").into_bytes();
    let paint = added(&chunk, &p.feed(&chunk));
    assert!(contains(
        &paint,
        &kitty::transmit_png(FIRST_IMAGE + 2, b"flat 40 18 (10, 10, 10)")
    ));
}

#[test]
fn a_card_without_frames_still_shows_early_and_hands_over_when_whole() {
    let fake = Fake::default();
    let mut p = Painter::new(10, 40, Route::KittyPlaceholders, fake).with_motion(Some(GREY));
    let id = card_image_id("a1b2c3");
    let first = b"[tile:a1b2c3 01/03] Signups\r\n";
    let paint = added(first, &p.feed(first));
    assert!(contains(&paint, &kitty::virtual_png(id, PNG, 20, 3)));
    assert!(contains(
        &paint,
        &kitty::place(id, 2, 20, 3, 2, Some(crop(18)))
    ));
    assert!(!contains(&paint, b"a=a,"), "no animation to run");
    let rest = b"[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
    let paint = added(rest, &p.feed(rest));
    assert!(contains(&paint, &kitty::delete_placement(id, 2)));
}

#[test]
fn a_card_drawn_again_is_swapped_in_place_under_its_image_id() {
    const DARK: &[u8] = b"\x89PNG whole card, dark";
    let fake = Fake::default();
    let mut p = Painter::new(10, 40, Route::KittyPlaceholders, fake).with_motion(Some(GREY));
    let id = card_image_id("a1b2c3");
    let _ = p.feed(BLOCK);
    let _ = p.tick(Duration::from_millis(50));
    p.pictures.png = Some(DARK.to_vec());
    p.pictures.redrawn = vec!["a1b2c3".into()];
    let swap = p.tick(Duration::from_millis(100));
    assert_eq!(
        swap,
        kitty::virtual_png(id, DARK, 20, 3),
        "the new picture under the same id, nothing else: {}",
        String::from_utf8_lossy(&swap)
    );
    assert_eq!(p.tick(Duration::from_millis(150)), b"", "sent once");
    // A card never sent is left for its block to send.
    p.pictures.redrawn = vec!["d4e5f6".into()];
    assert_eq!(p.tick(Duration::from_millis(200)), b"");
}

#[test]
fn a_card_drawn_again_mid_entrance_is_swapped_once_it_settles() {
    // The entrance reaches kitty before the block, so it plays over it.
    let mut p = sent_ahead(3);
    let id = card_image_id("a1b2c3");
    let _ = p.feed(BLOCK);
    p.pictures.png = Some(b"new".to_vec());
    p.pictures.redrawn = vec!["a1b2c3".into()];
    let during = p.tick(Duration::from_millis(60));
    assert!(!contains(&during, &kitty::virtual_png(id, b"new", 20, 3)));
    let mut later = Vec::new();
    for ms in (110..3000).step_by(50) {
        later.extend(p.tick(Duration::from_millis(ms)));
    }
    assert_eq!(
        later
            .windows(kitty::virtual_png(id, b"new", 20, 3).len())
            .filter(|w| *w == kitty::virtual_png(id, b"new", 20, 3).as_slice())
            .count(),
        1,
        "{}",
        String::from_utf8_lossy(&later)
    );
}

#[test]
fn the_hostile_corpus_adds_no_sequence_of_its_own_with_motion_on() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        for mut p in [animated(3), entering(3)] {
            let chunk = format!("[tile:a1b2c3 01/03] {e}\r\n");
            let out = p.feed(chunk.as_bytes());
            let mut paint = String::from_utf8_lossy(&added(chunk.as_bytes(), &out)).into_owned();
            for ms in [80, 100, 150, 2000] {
                paint.push_str(&String::from_utf8_lossy(&p.tick(Duration::from_millis(ms))));
            }
            assert!(
                !paint.contains("\u{1b}]") && !paint.contains('\u{7}') && !paint.contains('\u{9b}'),
                "{e:?}"
            );
            assert!(!paint.contains("pwned") && !paint.contains("evil"), "{e:?}");
        }
    }
}

#[test]
fn cells_a_moved_block_left_get_the_harness_text_back() {
    let mut p = painter(Route::KittyPlaceholders);
    let _ = p.feed(BLOCK);
    // The harness blanks the old first token and draws the block a row
    // lower, rewriting only the cells it changed, as OpenCode does.
    let chunk = [
        b"\x1b[1;1H".as_slice(),
        &[b' '; 19],
        b"\x1b[2;1H[tile:a1b2c3 01/03] Signups\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n",
    ]
    .concat();
    let paint = added(&chunk, &p.feed(&chunk));
    let at = paint
        .windows(10)
        .position(|w| w == b"\x1b[1;1H\x1b[0m")
        .expect("row 1 written back");
    let row: Vec<u8> = paint[at + 10..]
        .split(|b| *b == 0x1b)
        .next()
        .unwrap()
        .to_vec();
    assert!(
        !String::from_utf8_lossy(&row).contains('\u{10EEEE}'),
        "no placeholder left on the row the block left"
    );
    let id = card_image_id("a1b2c3");
    assert!(contains(
        &paint,
        kitty::placeholder_rows(id, 20, 3)[0].as_bytes()
    ));
}

#[test]
fn writing_back_cells_a_block_left_adds_no_sequence_from_the_hostile_corpus() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut written_back = 0;
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        let mut p = painter(Route::KittyPlaceholders);
        let first =
            format!("[tile:a1b2c3 01/03] {e}\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n");
        let _ = p.feed(first.as_bytes());
        // The block moves a row down and the corpus stays on the row it left.
        let chunk = format!(
            "\x1b[1;1H{}\x1b[2;1H[tile:a1b2c3 01/03] T\r\n[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n",
            " ".repeat(19)
        );
        let out = p.feed(chunk.as_bytes());
        let paint = String::from_utf8_lossy(&out[chunk.len()..]).into_owned();
        // An entry that clears the screen leaves nothing to write back.
        if paint.contains("\u{1b}[1;1H\u{1b}[0m") {
            written_back += 1;
        }
        assert!(
            !paint.contains("\u{1b}]") && !paint.contains('\u{7}') && !paint.contains('\u{9b}'),
            "{e:?}"
        );
    }
    assert!(
        written_back > 10,
        "{written_back} entries reached the write-back"
    );
}

#[test]
fn a_row_written_back_reads_as_the_harness_wrote_it() {
    for text in [
        "     Made-up example data for Nov 2025\u{2013}Oct 2026. Revenue trends up.",
        "\x1b[48;2;10;10;10m  \x1b[0m  Made-up  example \x1b[1mdata\x1b[0m end",
        "     a     b      c",
    ] {
        let mut harness = vt100::Parser::new(5, 80, 0);
        harness.process(b"\x1b[3;1H");
        harness.process(text.as_bytes());
        for start in [0u16, 5, 7] {
            let row: Vec<u8> = harness.screen().rows_formatted(start, 60).nth(2).unwrap();
            let mut term = vt100::Parser::new(5, 80, 0);
            term.process(b"\x1b[3;1H");
            term.process(text.as_bytes());
            // Scribble over the cells, then write the row back over them.
            term.process(format!("\x1b[3;{}H{}", start + 1, "#".repeat(60)).as_bytes());
            // As the painter writes it back: cleared, then the row.
            term.process(
                format!(
                    "\x1b[3;{}H\x1b[0m{}\x1b[3;{}H",
                    start + 1,
                    " ".repeat(60),
                    start + 1
                )
                .as_bytes(),
            );
            term.process(&row);
            // What shows: each cell's text, background and weight.
            let shown = |p: &vt100::Parser| -> Vec<(String, vt100::Color, bool)> {
                (0..80)
                    .map(|x| {
                        let c = p.screen().cell(2, x).unwrap();
                        let text = if c.has_contents() {
                            c.contents().to_string()
                        } else {
                            " ".to_string()
                        };
                        (text, c.bgcolor(), c.bold())
                    })
                    .collect()
            };
            assert_eq!(shown(&term), shown(&harness), "{text:?} from {start}");
        }
    }
}

#[test]
fn an_early_card_follows_its_block_when_its_top_scrolls_off() {
    let mut p = Painter::new(
        4,
        40,
        Route::KittyPlaceholders,
        Fake {
            rows: 6,
            ..Fake::default()
        },
    )
    .with_motion(Some(GREY));
    let id = card_image_id("a1b2c3");
    let first = b"[tile:a1b2c3 01/06] T\r\n[tile:a1b2c3 02/06]\r\n[tile:a1b2c3 03/06]\r\n";
    let paint = added(first, &p.feed(first));
    assert!(contains(
        &paint,
        &kitty::place(id, 2, 20, 4, 2, Some(crop(24)))
    ));
    // Two more rows scroll the first two off the top; the block is not yet
    // whole, and the card stays over it, cropped from its third row.
    let more = b"[tile:a1b2c3 04/06]\r\n[tile:a1b2c3 05/06]\r\n";
    let paint = added(more, &p.feed(more));
    assert!(
        !contains(&paint, &kitty::delete_placement(id, 2)),
        "not removed"
    );
    assert!(
        contains(
            &paint,
            &kitty::place(
                id,
                2,
                20,
                4,
                2,
                Some(kitty::Crop {
                    x: 0,
                    y: 12,
                    w: 40,
                    h: 24
                })
            )
        ),
        "{}",
        String::from_utf8_lossy(&paint)
    );
}

#[test]
fn a_blanked_cell_the_harness_does_not_rewrite_gets_its_text_back() {
    let mut p = painter(Route::KittyPlaceholders);
    let mut term = vt100::Parser::new(10, 40, 0);
    let typed = b"[tile:a1";
    term.process(&p.feed(typed));
    assert_eq!(term.screen().contents_between(0, 0, 0, 8).trim(), "");
    // The reply replaces the half-typed token, and the harness rewrites only
    // the cells whose letter changed: "[tile:a1" and "The example" share
    // the "e" in the fifth cell and the "a" in the seventh, as in OpenCode.
    let reply = b"\x1b[1;1HThe \x1b[1;6Hx\x1b[1;8Hmple";
    term.process(&p.feed(reply));
    assert_eq!(term.screen().contents_between(0, 0, 0, 11), "The example");
}

#[test]
fn writing_back_blanked_cells_adds_no_sequence_from_the_hostile_corpus() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/hostile/text.json"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut written_back = 0;
    for e in json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
    {
        let mut p = painter(Route::KittyPlaceholders);
        let _ = p.feed(format!("{e}\x1b[1;1H[tile:a1").as_bytes());
        // The half-typed token gives way to the corpus text left under it.
        let chunk = format!("\x1b[1;1H{e}");
        let out = p.feed(chunk.as_bytes());
        let paint = String::from_utf8_lossy(&added(chunk.as_bytes(), &out)).into_owned();
        if paint.contains("\u{1b}[1;1H\u{1b}[0m") {
            written_back += 1;
        }
        assert!(
            !paint.contains("\u{1b}]") && !paint.contains('\u{7}') && !paint.contains('\u{9b}'),
            "{e:?}"
        );
    }
    assert!(
        written_back > 10,
        "{written_back} entries reached the write-back"
    );
}

#[test]
fn a_block_taller_than_the_harness_viewport_draws_its_visible_band() {
    // OpenCode keeps its tab bar on screen row 0 and scrolls its messages
    // in the rows under it, so the 30-row block never reaches row 0 and is
    // never on screen whole.
    let fake = Fake {
        rows: 30,
        ..Fake::default()
    };
    let mut p = Painter::new(20, 60, Route::KittyPrecut, fake);
    p.feed(b"OpenCode  session tab bar\x1b[2;20r\x1b[2;1H");
    for row in 1..=30 {
        let line = format!("[tile:a1b2c3 {row:02}/30]\r\n");
        p.feed(line.as_bytes());
    }
    assert_eq!(
        p.pictures.bands.last(),
        Some(&(12, 18)),
        "rows 13 to 30 are on screen and drawn as one band"
    );
    assert!(p.blanked.is_empty(), "no token left blank: {:?}", p.blanked);
}

#[test]
fn a_block_seen_only_in_part_is_never_whole_and_never_placed() {
    let fake = Fake {
        rows: 30,
        ..Fake::default()
    };
    let mut p = Painter::new(20, 60, Route::KittyPrecut, fake).with_trace();
    p.feed(b"OpenCode  session tab bar\x1b[2;20r\x1b[2;1H");
    // A row every 60 ms, as OpenCode types.
    for row in 1..=29u64 {
        p.set_now(Duration::from_millis(row * 60));
        let line = format!("[tile:a1b2c3 {row:02}/30]\r\n");
        let paint = added(line.as_bytes(), &p.feed(line.as_bytes()));
        assert!(
            !contains(&paint, b"\x1b_G"),
            "row {row}: the 19-row viewport never holds the 30 rows, so no card is cut to it"
        );
    }
    assert!(!p.whole.contains("a1b2c3"), "row 30 was never typed");
    assert!(
        !p.take_trace().iter().any(|l| l.contains("whole")),
        "no trace line calls it whole"
    );
    assert!(!p.strips.contains_key("a1b2c3"));
    let rows: Vec<u16> = p.blanked.iter().map(|b| b.0).collect();
    assert_eq!(
        rows,
        (1..=18).collect::<Vec<u16>>(),
        "every token on screen is blank"
    );
}

#[test]
fn no_painter_image_id_is_a_cards_image_id() {
    // Every card_image_id has the low bit of each byte set; a painter id
    // that did too would, once deleted, take a card's picture with it.
    let mut p = painter(Route::KittyPlaceholders);
    let first = p.take_image_id();
    assert_eq!(first, FIRST_IMAGE);
    let mut id = first;
    loop {
        assert!((FIRST_IMAGE..=0x7f_ffff).contains(&id), "{id:#x}");
        assert_ne!(id & 0x01_0101, 0x01_0101, "{id:#x} is a card's id");
        id = p.take_image_id();
        if id == first {
            break;
        }
    }
    assert_eq!(card_image_id("510101") & 0x01_0101, 0x01_0101);
}

// The pre-cut route: the conversation's background, and the card growing
// with its block.

const DARK: vt100::Color = vt100::Color::Rgb(10, 10, 10);

/// Asserts every cell of screen rows `ys`, columns `0..cols`, of the
/// user's terminal `term` is on the dark conversation's colour.
fn all_dark(term: &vt100::Parser, ys: std::ops::Range<u16>, cols: u16, when: &str) {
    for y in ys {
        for x in 0..cols {
            let bg = term.screen().cell(y, x).map(vt100::Cell::bgcolor);
            assert_eq!(bg, Some(DARK), "{when}: row {y}, column {x}");
        }
    }
}

#[test]
fn a_typed_row_never_shows_on_the_default_background_in_a_dark_conversation() {
    // OpenCode paints its screen #0a0a0a and types a fenced block on the
    // default background, here from column 0 of the first row, so no
    // painted cell sits beside it.
    let fake = Fake {
        rows: 6,
        ..Fake::default()
    };
    let mut p = Painter::new(12, 40, Route::KittyPrecut, fake);
    // What the user's terminal shows; kitty pictures are not modelled, so
    // a cell under a card shows the blank under it.
    let mut term = vt100::Parser::new(12, 40, 0);
    let screen = b"\x1b[48;2;10;10;10m\x1b[2J\x1b[H";
    term.process(&p.feed(screen));
    for row in 1..=6u16 {
        let line = format!("\x1b[{row};1H\x1b[0m\x1b[K[tile:a1b2c3 {row:02}/06]");
        let out = p.feed(line.as_bytes());
        assert!(
            out.starts_with(&[SYNC_ON, line.as_bytes()].concat()),
            "row {row}: the harness's row and its blank share one update"
        );
        term.process(&out);
        all_dark(&term, 0..row, 20, &format!("after row {row}"));
    }
    assert!(p.whole.contains("a1b2c3"));
    // The harness clears to the default and types the next card from the
    // first row: the conversation's colour is remembered.
    let fake = Fake::default();
    let mut p = Painter::new(12, 40, Route::KittyPrecut, fake);
    let mut term = vt100::Parser::new(12, 40, 0);
    term.process(&p.feed(b"\x1b[48;2;10;10;10m\x1b[2J\x1b[H\x1b[0m[tile:a1"));
    let out = p.feed(b"\x1b[0m\x1b[2J\x1b[H[tile:a1b2c3 01/03]");
    term.process(&out);
    all_dark(&term, 0..1, 20, "a fresh screen");
}

#[test]
fn claude_codes_default_background_stays_the_default() {
    let mut p = Painter::new(12, 40, Route::KittyPrecut, Fake::default());
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(contains(
        &paint,
        &[b"\x1b[1;1H\x1b[0m".as_slice(), &[b' '; 20]].concat()
    ));
    assert_eq!(p.conversation, None);
}

#[test]
fn precut_a_tall_card_shows_whole_from_its_first_row_and_typing_changes_nothing() {
    let fake = Fake {
        rows: 48,
        ..Fake::default()
    };
    let mut p = Painter::new(60, 40, Route::KittyPrecut, fake).with_trace();
    let line = b"[tile:a1b2c3 01/48]\r\n";
    let paint = added(line, &p.feed(line));
    assert!(
        contains(
            &paint,
            &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 48, None, 1)
        ),
        "the whole card over the rows its block will take"
    );
    assert!(p.pictures.bands.is_empty(), "no band cut");
    for y in 1..48u16 {
        assert!(
            contains(&paint, &blank_row(y, 20)),
            "screen row {y}, not typed yet, is blank under the card"
        );
    }
    for row in 2..=48u16 {
        let line = format!("[tile:a1b2c3 {row:02}/48]\r\n");
        let paint = added(line.as_bytes(), &p.feed(line.as_bytes()));
        assert!(
            !contains(&paint, b"\x1b_G"),
            "row {row}: nothing placed again or deleted"
        );
        assert!(
            contains(&paint, &blank_row(row - 1, 20)),
            "row {row}: its token is blanked"
        );
        assert_eq!(p.whole.contains("a1b2c3"), row == 48, "row {row}");
    }
    assert_eq!(p.strips["a1b2c3"].len(), 1);
    assert!(p.blanked.is_empty(), "{:?}", p.blanked);
    let trace = p.take_trace();
    let whole: Vec<&String> = trace.iter().filter(|l| l.contains("whole")).collect();
    assert_eq!(whole.len(), 1, "{trace:#?}");
    assert!(whole[0].contains("a1b2c3 whole: rows 1 to 48"), "{whole:?}");
    // Unrelated output once it is whole changes nothing.
    assert_eq!(p.feed(b"done"), b"done");
}

#[test]
fn precut_a_dashboard_and_the_card_after_it_each_show_whole_at_once() {
    let fake = Fake {
        rows: 12,
        ..Fake::default()
    };
    let mut p = Painter::new(40, 40, Route::KittyPrecut, fake);
    // The harness writes the dashboard's block four rows at a time.
    for first in [1u16, 5, 9] {
        let mut chunk = String::new();
        for r in first..first + 4 {
            let _ = write!(chunk, "[tile:a1b2c3 {r:02}/12]\r\n");
        }
        let paint = added(chunk.as_bytes(), &p.feed(chunk.as_bytes()));
        if first == 1 {
            assert!(contains(
                &paint,
                &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 12, None, 1)
            ));
        } else {
            assert!(!contains(&paint, b"\x1b_G"), "rows {first} on");
        }
    }
    assert!(p.whole.contains("a1b2c3"));
    let dashboard = p.strips["a1b2c3"].clone();
    assert_eq!(dashboard.len(), 1);
    // The next card in the reply shows whole under it; the dashboard stays.
    for row in 1..=12u16 {
        let line = format!("[tile:{SECOND} {row:02}/12]\r\n");
        let paint = added(line.as_bytes(), &p.feed(line.as_bytes()));
        if row == 1 {
            assert!(contains(
                &paint,
                &kitty::show_png_over(FIRST_IMAGE + 2, PNG, 20, 12, None, 2)
            ));
        } else {
            assert!(!contains(&paint, b"\x1b_G"), "row {row}");
        }
    }
    assert!(p.whole.contains(SECOND));
    assert_eq!(p.strips[SECOND].len(), 1);
    assert_eq!(p.strips[SECOND][0].y, 12);
    assert_eq!(p.strips["a1b2c3"], dashboard);
}

#[test]
fn precut_a_card_typed_into_a_short_screen_is_placed_again_as_one_strip() {
    let fake = Fake {
        rows: 6,
        ..Fake::default()
    };
    let mut p = Painter::new(6, 40, Route::KittyPrecut, fake);
    for row in 1..=5u16 {
        let line = format!("[tile:a1b2c3 {row:02}/06]\r\n");
        let paint = added(line.as_bytes(), &p.feed(line.as_bytes()));
        if row == 1 {
            assert!(contains(
                &paint,
                &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 6, None, 1)
            ));
        }
    }
    assert!(p.pictures.bands.is_empty(), "the whole card, no band cut");
    let old: Vec<u32> = p.strips["a1b2c3"].iter().map(|s| s.image).collect();
    // The harness adds a line under the block, scrolling row 1 off the
    // six-row screen.
    let paint = added(b"\r\n", &p.feed(b"\r\n"));
    let now = &p.strips["a1b2c3"];
    assert_eq!(now.len(), 1);
    assert_eq!((now[0].y, now[0].lo, now[0].hi), (0, 2, 6));
    assert_eq!(p.pictures.bands, [(1, 5)]);
    assert!(contains(&paint, b"\x1b_Ga=T"));
    for image in &old {
        assert!(
            !contains(&paint, &kitty::delete(*image)),
            "the old strip stays while the new one shows"
        );
    }
    let paint = p.tick(Duration::from_millis(50));
    for image in &old {
        assert!(
            contains(&paint, &kitty::delete(*image)),
            "gone a tick later"
        );
    }
}

#[test]
fn precut_a_typed_card_that_scrolls_each_row_is_placed_again_once_a_tick() {
    let fake = Fake {
        rows: 12,
        ..Fake::default()
    };
    let mut p = Painter::new(30, 40, Route::KittyPrecut, fake).with_motion(None);
    let placements = |out: &[u8]| out.windows(6).filter(|w| *w == b"\x1b_Ga=T").count();
    // The harness types each row on screen row 17 and scrolls the screen up
    // a row after it, so each row typed moves the block up a row, with room
    // under it for all 12. The first row at 0 ms, the rest 20 ms apart from
    // 60 ms, a tick every 50 ms: the card is placed again, whole, at most
    // once a tick.
    let mut rows = 1..=11u16;
    let mut per_tick = vec![0usize; 6];
    for t in (0..=250u64).step_by(10) {
        let now = Duration::from_millis(t);
        let mut out = Vec::new();
        if t % 50 == 0 && t > 0 {
            out.extend(p.tick(now));
        }
        if (t == 0 || (t >= 60 && t % 20 == 0))
            && let Some(row) = rows.next()
        {
            p.set_now(now);
            let line = format!("\x1b[18;1H[tile:a1b2c3 {row:02}/12]\x1b[S");
            let paint = added(line.as_bytes(), &p.feed(line.as_bytes()));
            if row == 3 {
                assert_eq!(placements(&paint), 0, "row 3 waits for the tick");
                assert!(
                    contains(&paint, &[b' '; 20]),
                    "its token is blanked meanwhile"
                );
            }
            out.extend(paint);
        }
        per_tick[usize::try_from(t / 50).unwrap()] += placements(&out);
    }
    assert!(rows.next().is_none(), "11 rows typed");
    assert!(per_tick.iter().all(|n| *n <= 1), "{per_tick:?}");
    assert!(per_tick.iter().sum::<usize>() > 3, "{per_tick:?}");
    assert!(
        p.pictures.bands.is_empty(),
        "every placement is the whole card: {:?}",
        p.pictures.bands
    );
    // A scroll with no new row waits for the tick too.
    p.set_now(Duration::from_millis(260));
    assert_eq!(placements(&added(b"\x1b[S", &p.feed(b"\x1b[S"))), 0);
    let paint = p.tick(Duration::from_millis(300));
    assert_eq!(placements(&paint), 1);
    let strip = p.strips["a1b2c3"][0];
    assert!(contains(&paint, format!("i={},", strip.image).as_bytes()));
    // The last row, typed right under the block and scrolled with it, makes
    // the card whole: the crisp card goes up at once, though the card was
    // placed again this tick.
    p.set_now(Duration::from_millis(310));
    let line = b"\x1b[17;1H[tile:a1b2c3 12/12]\x1b[S";
    let paint = added(line, &p.feed(line));
    let crisp = p.strips["a1b2c3"][0];
    assert!(
        shows(&paint, crisp.image, PNG, 20, 12),
        "{}",
        String::from_utf8_lossy(&paint)
    );
    assert!(p.whole.contains("a1b2c3"));
}

/// The rows each picture `paint` places covers, in order.
fn rows_placed(paint: &[u8]) -> Vec<u16> {
    String::from_utf8_lossy(paint)
        .split("\x1b_G")
        .skip(1)
        .filter(|cmd| cmd.starts_with("a=T") || cmd.starts_with("a=p"))
        .filter_map(|cmd| {
            let keys = cmd.split([';', '\x1b']).next()?;
            keys.split(',')
                .find_map(|kv| kv.strip_prefix("r="))?
                .parse()
                .ok()
        })
        .collect()
}

#[test]
fn precut_a_block_with_no_room_under_it_shows_its_card_only_whole() {
    let fake = Fake {
        rows: 23,
        ..Fake::default()
    };
    let mut p = Painter::new(30, 40, Route::KittyPrecut, fake).with_motion(None);
    let mut term = vt100::Parser::new(30, 40, 0);
    // The conversation fills the screen and a five-row input box sits at its
    // foot, as in Claude Code after a few exchanges: each row is typed above
    // the box, which is drawn again under it and scrolls the screen a row.
    term.process(&p.feed(&b"\r\n".repeat(29)));
    let mut first = None;
    for row in 1..=23u16 {
        p.set_now(Duration::from_millis(u64::from(row) * 120));
        let mut chunk = format!("\x1b[26;1H\x1b[J[tile:a1b2c3 {row:02}/23]");
        for line in ["+------+", "| > ask", "|", "|", "+------+"] {
            chunk.push_str("\r\n");
            chunk.push_str(line);
        }
        let out = p.feed(chunk.as_bytes());
        term.process(&out);
        for r in rows_placed(&added(chunk.as_bytes(), &out)) {
            assert_eq!(
                r, 23,
                "row {row}: a card placed shows every row of its block"
            );
            first.get_or_insert(row);
        }
        assert!(
            !term.screen().contents().contains("[tile"),
            "row {row}: every token typed is blank"
        );
    }
    // Typed at screen row 24 with the box under it, row 18 is the first with
    // room under the block's first row for all 23.
    assert_eq!(
        first,
        Some(18),
        "placed whole once the room covers the block"
    );
    assert!(p.whole.contains("a1b2c3"));
}

/// Each entrance frame trace lines say was placed, with its screen row.
fn frames_and_rows(trace: &[String]) -> Vec<(u16, u16)> {
    trace
        .iter()
        .filter_map(|l| l.split(" placed frame ").nth(1))
        .filter_map(|rest| {
            let (frame, row) = rest.split_once(" at screen row ")?;
            Some((frame.parse().ok()?, row.parse().ok()?))
        })
        .collect()
}

/// A pre-cut painter on a 30-row screen with a 23-row card whose frame
/// bands are over a tick's budget, its 12 entrance frames sent ahead, the
/// screen filled by 29 lines.
fn waiting_for_room() -> Painter<Fake> {
    let fake = Fake {
        frames: 14,
        rows: 23,
        huge: true,
        ..Fake::default()
    };
    let mut p = Painter::new(30, 40, Route::KittyPrecut, fake)
        .with_motion(None)
        .with_trace();
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    let _ = p.feed(&b"\r\n".repeat(29));
    p
}

#[test]
fn precut_a_block_that_waited_for_room_plays_its_entrance_in_one_place() {
    let mut p = waiting_for_room();
    // Each row is typed above a five-row input box at the foot of the full
    // screen, 120 ms apart, and scrolls the screen a row; a tick every 50 ms.
    let mut rows = 1..=23u16;
    for t in (100..=3600u64).step_by(10) {
        let now = Duration::from_millis(t);
        if t % 50 == 0 {
            let _ = p.tick(now);
        }
        if t % 120 == 0
            && let Some(row) = rows.next()
        {
            p.set_now(now);
            let mut chunk = format!("\x1b[26;1H\x1b[J[tile:a1b2c3 {row:02}/23]");
            for line in ["+------+", "| > ask", "|", "|", "+------+"] {
                chunk.push_str("\r\n");
                chunk.push_str(line);
            }
            let _ = p.feed(chunk.as_bytes());
        }
    }
    assert!(rows.next().is_none(), "23 rows typed");
    let trace = p.take_trace();
    let whole = trace.iter().position(|l| l.contains("a1b2c3 whole:"));
    let first = trace.iter().position(|l| l.contains(" placed "));
    assert!(
        whole < first,
        "nothing placed while the block moves: {trace:#?}"
    );
    let placed = frames_and_rows(&trace);
    assert_eq!(
        placed.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
        (1..=12).collect::<Vec<_>>(),
        "{trace:#?}"
    );
    assert!(
        placed.iter().all(|(_, row)| *row == placed[0].1),
        "every frame at one screen row: {placed:?}"
    );
    assert!(
        !trace.iter().any(|l| l.contains("entrance dropped")),
        "{trace:#?}"
    );
}

#[test]
fn precut_a_block_that_waited_for_room_enters_a_tick_after_a_row_typed_in_place() {
    let mut p = waiting_for_room();
    // Ten rows typed at the foot of the screen, each scrolling it a row.
    for row in 1..=10u16 {
        p.set_now(Duration::from_millis(50 + u64::from(row) * 20));
        let line = format!("[tile:a1b2c3 {row:02}/23]\r\n");
        let _ = p.feed(line.as_bytes());
    }
    assert!(p.strips.is_empty(), "no room yet");
    let trace = p.take_trace();
    let waiting: Vec<&String> = trace
        .iter()
        .filter(|l| l.contains("waiting for room"))
        .collect();
    assert_eq!(waiting.len(), 1, "{trace:#?}");
    assert!(
        waiting[0].ends_with("a1b2c3 waiting for room: needs 23 rows, has 2"),
        "{waiting:?}"
    );
    // The reply scrolls the block up to screen row 4, with room for all 23.
    p.set_now(Duration::from_millis(300));
    let _ = p.feed(&b"\r\n".repeat(15));
    assert!(p.strips.is_empty(), "it has just moved");
    assert!(
        p.take_trace()
            .iter()
            .any(|l| l
                .ends_with("a1b2c3 has room at screen row 4; held until its card stops moving")),
    );
    // The rest is typed in place under it, 120 ms apart.
    let mut rows = 11..=23u16;
    for t in (310..=2500u64).step_by(10) {
        let now = Duration::from_millis(t);
        if t % 50 == 0 {
            let _ = p.tick(now);
        }
        if (t - 310) % 120 == 0
            && let Some(row) = rows.next()
        {
            p.set_now(now);
            let line = format!("\x1b[{};1H[tile:a1b2c3 {row:02}/23]", row + 4);
            let _ = p.feed(line.as_bytes());
            if row == 11 {
                assert!(p.strips.is_empty(), "row 11 typed in place: a tick to go");
            }
        }
    }
    let trace = p.take_trace();
    let whole = trace.iter().position(|l| l.contains("a1b2c3 whole:"));
    let starts = trace.iter().position(|l| l.contains("entrance starts"));
    assert!(starts < whole, "it entered before typing ended: {trace:#?}");
    // Row 11 at 310 ms; the tick at 350 ms is 40 ms on, the one at 400 ms
    // a whole tick.
    assert!(
        trace[starts.unwrap()].starts_with("    400.0 "),
        "{trace:#?}"
    );
    let placed = frames_and_rows(&trace);
    assert_eq!(placed.len(), 12, "{placed:?}");
    assert!(placed.iter().all(|(_, row)| *row == 4), "{placed:?}");
}

#[test]
fn precut_an_interrupted_block_gives_the_harness_lines_under_it_back() {
    let fake = Fake {
        rows: 20,
        ..Fake::default()
    };
    let mut p = Painter::new(40, 60, Route::KittyPrecut, fake);
    let mut term = vt100::Parser::new(40, 60, 0);
    let text =
        |term: &vt100::Parser, y: u16| term.screen().rows(0, 60).nth(usize::from(y)).unwrap();
    // Claude Code types rows 1 to 4 of 20 from screen row 2, a row every
    // 120 ms, its spinner two rows under the last.
    for row in 1..=4u16 {
        p.set_now(Duration::from_millis(u64::from(row) * 120));
        let chunk = format!(
            "\x1b[{};1H\x1b[J[tile:a1b2c3 {row:02}/20]\r\n\r\n* Thinking {row}s (esc to interrupt)",
            row + 2
        );
        term.process(&p.feed(chunk.as_bytes()));
    }
    assert_eq!(
        p.strips["a1b2c3"].last().unwrap().hi,
        20,
        "the whole card while it is typed"
    );
    // The user presses Esc: Claude Code keeps the four rows and writes its
    // Interrupted line and input box under them, which stay under the card
    // until 1.5 s after row 4.
    p.set_now(Duration::from_millis(600));
    let chunk = b"\x1b[7;1H\x1b[J  Interrupted by user\r\n\r\n+------+\r\n| > next\r\n+------+";
    term.process(&p.feed(chunk));
    assert!(!text(&term, 6).contains("Interrupted"), "under the card");
    term.process(&p.tick(Duration::from_millis(1950)));
    assert!(
        !text(&term, 6).contains("Interrupted"),
        "1.47 s after row 4"
    );
    term.process(&p.tick(Duration::from_millis(1980)));
    assert_eq!(
        p.strips["a1b2c3"].last().unwrap().hi,
        4,
        "the card cut to the typed rows"
    );
    assert!(p.blanked.is_empty(), "{:?}", p.blanked);
    assert!(text(&term, 6).contains("Interrupted by user"));
    assert!(text(&term, 9).contains("| > next"));
    // A resize, and Claude Code drawing the screen again, leave it cut.
    let _ = p.resize(40, 70);
    p.set_now(Duration::from_millis(2000));
    let mut chunk = String::from("\x1b[H\x1b[2J");
    for row in 1..=4u16 {
        let _ = write!(chunk, "\x1b[{};1H[tile:a1b2c3 {row:02}/20]", row + 2);
    }
    chunk.push_str("\x1b[7;1H  Interrupted by user\r\n\r\n+------+\r\n| > next\r\n+------+");
    let paint = added(chunk.as_bytes(), &p.feed(chunk.as_bytes()));
    assert_eq!(
        rows_placed(&paint),
        [4],
        "the typed rows only, after a resize"
    );
    // So does a switch to the alternate screen and back.
    p.set_now(Duration::from_millis(2010));
    let _ = p.feed(b"\x1b[?1049h\x1b[2J\x1b[Hless");
    p.set_now(Duration::from_millis(2020));
    let chunk = b"\x1b[?1049l";
    let paint = added(chunk, &p.feed(chunk));
    assert_eq!(
        rows_placed(&paint),
        [4],
        "the typed rows only, back on screen"
    );
    assert!(p.blanked.is_empty(), "{:?}", p.blanked);
    // So does a redraw that reaches the painter a row at a time: rows it
    // has seen before are not new rows.
    let mut chunks = vec![String::from("\x1b[H\x1b[2J")];
    for row in 1..=4u16 {
        chunks.push(format!("\x1b[{};1H[tile:a1b2c3 {row:02}/20]", row + 2));
    }
    chunks.push(String::from(
        "\x1b[7;1H  Interrupted by user\r\n\r\n+------+\r\n| > next\r\n+------+",
    ));
    for (i, chunk) in chunks.iter().enumerate() {
        p.set_now(Duration::from_millis(2030 + 10 * i as u64));
        let paint = added(chunk.as_bytes(), &p.feed(chunk.as_bytes()));
        assert!(
            rows_placed(&paint).iter().all(|r| *r <= 4),
            "the typed rows only, redrawn a row at a time: {:?}",
            rows_placed(&paint)
        );
    }
    assert_eq!(p.strips["a1b2c3"].last().unwrap().hi, 4);
    // Rows typed again reach the whole card again.
    p.set_now(Duration::from_millis(2100));
    let chunk = b"\x1b[7;1H\x1b[J[tile:a1b2c3 05/20]";
    let paint = added(chunk, &p.feed(chunk));
    assert_eq!(rows_placed(&paint), [16]);
    assert_eq!(p.strips["a1b2c3"].last().unwrap().hi, 20);
}

#[test]
fn precut_a_ticking_line_under_the_block_never_cuts_its_card() {
    let fake = Fake {
        rows: 20,
        ..Fake::default()
    };
    let mut p = Painter::new(30, 60, Route::KittyPrecut, fake);
    // OpenCode types each row into its "Build" line, which moves down under
    // it and ticks between rows; a read can end with the next row half
    // typed.
    for row in 1..=19u16 {
        let at = u64::from(row) * 100;
        for (dt, chunk) in [
            (
                0,
                format!("\x1b[{row};1H\x1b[J[tile:a1b2\r\nBuild {row}.0s"),
            ),
            (20, format!("\x1b[{};1H\x1b[2KBuild {row}.5s", row + 1)),
            (
                40,
                format!("\x1b[{row};1H\x1b[J[tile:a1b2c3 {row:02}/20]\r\nBuild {row}.6s"),
            ),
        ] {
            let now = Duration::from_millis(at + dt);
            let mut out = p.tick(now);
            p.set_now(now);
            out.extend(added(chunk.as_bytes(), &p.feed(chunk.as_bytes())));
            for r in rows_placed(&out) {
                assert_eq!(r, 20, "row {row} at +{dt} ms: the whole card");
            }
            let hi = p.strips.get("a1b2c3").map(|s| s.last().unwrap().hi);
            assert!(hi.is_none_or(|hi| hi == 20), "row {row}: {hi:?}");
        }
    }
}

#[test]
fn precut_rows_not_typed_yet_are_blanked_and_given_back() {
    let mut p = Painter::new(10, 60, Route::KittyPrecut, Fake::default());
    let mut term = vt100::Parser::new(10, 60, 0);
    // OpenCode's "Build" line sits right under the row being typed.
    let chunk = b"[tile:a1b2c3 01/03] Signups\r\nBuild 12.4s";
    let paint = added(chunk, &p.feed(chunk));
    term.process(&[chunk.as_slice(), &paint].concat());
    assert!(
        contains(
            &paint,
            &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 3, None, 1)
        ),
        "the whole card, as wide as ever: the line under it does not cut it"
    );
    assert_eq!(p.blanked, [(1, 0, 20), (2, 0, 20)]);
    let text =
        |term: &vt100::Parser, y: u16| term.screen().rows(0, 60).nth(usize::from(y)).unwrap();
    assert_eq!(
        text(&term, 1).trim(),
        "",
        "the Build line is under the card"
    );
    // The harness erases the block before typing on: the card goes and its
    // Build line comes back.
    let chunk = b"\x1b[1;1H\x1b[2K";
    let paint = added(chunk, &p.feed(chunk));
    term.process(&[chunk.as_slice(), &paint].concat());
    assert_eq!(p.blanked, []);
    assert_eq!(text(&term, 1).trim(), "Build 12.4s");
    // Typed again in full, the Build line pushed down a row each time.
    let mut p = Painter::new(10, 60, Route::KittyPrecut, Fake::default());
    let mut term = vt100::Parser::new(10, 60, 0);
    for (row, chunk) in [
        b"[tile:a1b2c3 01/03] Signups\r\nBuild 12.4s".as_slice(),
        b"\r[tile:a1b2c3 02/03]\x1b[K\r\nBuild 12.4s",
        b"\r[tile:a1b2c3 03/03]\x1b[K\r\nBuild 12.4s",
    ]
    .into_iter()
    .enumerate()
    {
        let paint = added(chunk, &p.feed(chunk));
        term.process(&[chunk, &paint].concat());
        assert_eq!(
            p.blanked.len(),
            2 - row,
            "row {}: the rows still to come",
            row + 1
        );
    }
    assert!(p.whole.contains("a1b2c3"));
    assert_eq!(p.strips["a1b2c3"].len(), 1, "one picture all along");
    for y in 0..3usize {
        let under: String = term.screen().rows(0, 20).nth(y).unwrap();
        assert_eq!(under.trim(), "", "row {y} under the card's 20 columns");
    }
    assert_eq!(text(&term, 3).trim(), "Build 12.4s");
}

#[test]
fn precut_no_update_deletes_a_picture_it_replaces() {
    let fake = Fake {
        rows: 23,
        frames: 6,
        ..Fake::default()
    };
    let mut p = Painter::new(30, 40, Route::KittyPrecut, fake).with_motion(None);
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(40));
    // A full screen: each row typed scrolls the card up while its entrance
    // plays, so it is replaced both for frames and for moves.
    let _ = p.feed(&b"\r\n".repeat(29));
    let on_screen = |p: &Painter<Fake>| -> HashSet<u32> {
        p.strips.values().flatten().map(|s| s.image).collect()
    };
    let mut left: HashMap<u32, usize> = HashMap::new();
    let mut shown: HashSet<u32> = HashSet::new();
    let mut deletes = 0;
    let mut rows = 1..=23u16;
    for (step, t) in (50..=1000u64).step_by(10).enumerate() {
        let before = on_screen(&p);
        let now = Duration::from_millis(t);
        let mut out = Vec::new();
        if t % 50 == 0 {
            out.extend(p.tick(now));
        }
        if t % 20 == 0
            && let Some(row) = rows.next()
        {
            p.set_now(now);
            let line = format!("[tile:a1b2c3 {row:02}/23]\r\n");
            out.extend(p.feed(line.as_bytes()));
        }
        let after = on_screen(&p);
        for gone in before.difference(&after) {
            left.insert(*gone, step);
        }
        shown.extend(after.iter().copied());
        for image in deleted(&out) {
            deletes += 1;
            if shown.contains(&image) {
                assert!(
                    !after.contains(&image) && left.get(&image).is_some_and(|s| *s < step),
                    "image {image} deleted at {t} ms in the update that replaced it"
                );
            }
        }
    }
    assert!(rows.next().is_none());
    assert!(p.whole.contains("a1b2c3"));
    assert!(deletes > 0, "replaced pictures do go");
    assert!(p.doomed.is_empty(), "and none is left waiting");
}

#[test]
fn precut_a_whole_card_in_two_strips_goes_up_as_one() {
    let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default());
    let _ = p.feed(BLOCK);
    // The harness redraws the block's last row: the card loses it a moment,
    // then gets it back.
    let _ = p.feed(b"\x1b[3;1H\x1b[2K");
    assert_eq!(p.strips["a1b2c3"][0].hi, 2);
    let chunk = b"\x1b[3;1H[tile:a1b2c3 03/03]";
    let paint = added(chunk, &p.feed(chunk));
    let strips = &p.strips["a1b2c3"];
    assert_eq!(strips.len(), 1, "one picture, not a strip added under it");
    assert_eq!((strips[0].lo, strips[0].hi), (1, 3));
    assert!(shows(&paint, strips[0].image, PNG, 20, 3));
}

/// `OpenCode` streaming block row `row` of 3 to screen row `y` (from 1) at
/// column 6, in its own update and colours, as `OpenCode` 1.19.3 wrote it in
/// Warp during a diagnosis of its pre-cut route.
fn opencode_row(y: u16, row: u16) -> Vec<u8> {
    let title = if row == 1 { " Signups dipped" } else { "" };
    format!(
        "\x1b[?2026h\x1b[?25l\x1b[{y};6H\x1b[38;2;238;238;238m\x1b[48;2;10;10;10m\
         [tile:a1b2c3 {row:02}/03]{title}\x1b[0m\x1b[?25h\x1b[?2026l"
    )
    .into_bytes()
}

/// A painter after `OpenCode` streamed a 3-row block a row at a time, its
/// whole card placed once.
fn opencode_streams_a_block() -> Painter<Fake> {
    let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default()).with_trace();
    for row in 1..=3u16 {
        let _ = p.feed(&opencode_row(row + 1, row));
    }
    assert!(p.whole.contains("a1b2c3"));
    assert_eq!(p.strips["a1b2c3"].len(), 1, "the whole card, placed once");
    p
}

/// The last trace line about card `id`.
fn last_about<'a>(trace: &'a [String], id: &str) -> &'a str {
    trace
        .iter()
        .rev()
        .find(|l| l.contains(&format!(" {id} ")))
        .map_or("", String::as_str)
}

#[test]
fn precut_an_opencode_fenced_block_keeps_its_card_when_the_reply_ends() {
    let mut p = opencode_streams_a_block();
    let image = p.strips["a1b2c3"][0].image;
    // OpenCode shows a fenced block's rows as code and never writes them
    // again: the reply ends with its last line and the Build line under it.
    let end = b"\x1b[?2026h\x1b[?25l\x1b[6;6H\x1b[38;2;238;238;238m\x1b[48;2;10;10;10m\
                The W3 dip is marked.\x1b[0m\x1b[8;6H\x1b[38;2;92;156;245m\
                \x1b[48;2;10;10;10m\xe2\x96\xa3 Build\x1b[0m\x1b[?25h\x1b[?2026l";
    let out = p.feed(end);
    assert!(!deleted(&out).contains(&image), "the card stays");
    assert_eq!(p.strips["a1b2c3"][0].image, image);
}

#[test]
fn precut_an_opencode_block_drawn_again_as_links_is_no_block() {
    let mut p = opencode_streams_a_block();
    let image = p.strips["a1b2c3"][0].image;
    // OpenCode's markdown draws a plain [tile:] line again as a link: teal,
    // underlined, its brackets gone, two spaces where they were. The rows
    // hold no token, so the card goes, which is why the block reaches the
    // model inside a fence (ADR-052).
    let mut links = b"\x1b[?2026h\x1b[?25l".to_vec();
    for row in 1..=3u16 {
        let y = row + 1;
        links.extend(
            format!(
                "\x1b[{y};6H\x1b[38;2;86;182;194m\x1b[48;2;10;10;10m\x1b[4mtile:a1b2c3 {row:02}/03\
                 \x1b[0m\x1b[{y};23H\x1b[38;2;255;255;255m\x1b[48;2;10;10;10m  \x1b[0m"
            )
            .into_bytes(),
        );
    }
    links.extend_from_slice(b"\x1b[?25h\x1b[?2026l");
    let out = p.feed(&links);
    assert!(deleted(&out).contains(&image), "the card is deleted");
    assert!(!p.strips.contains_key("a1b2c3"));
    let trace = p.take_trace();
    assert!(
        last_about(&trace, "a1b2c3")
            .ends_with("a1b2c3 copy lost at screen row 1; pictures deleted"),
        "{trace:#?}"
    );
}

#[test]
fn precut_a_card_scrolled_off_the_foot_says_so_in_the_trace() {
    let mut p = painter(Route::KittyPrecut).with_trace();
    let _ = p.feed(BLOCK);
    // An editor holds the screen's last two rows, under the conversation's
    // eight. The harness inserts lines at the conversation's top, as a pane
    // scrolled back does, until the card's block sits on its last rows, then
    // two more, each pushing the card down a row under the editor, until too
    // little of it is left.
    let _ = p.feed(b"\x1b[9;1H> editor\x1b[10;1HBuild\x1b[1;8r\x1b[1;1H\x1b[5L");
    assert!(p.strips.contains_key("a1b2c3"));
    for _ in 0..2 {
        let _ = p.feed(b"\x1b[1;1H\x1b[L");
    }
    assert!(p.parser.screen().contents().ends_with("> editor\nBuild"));
    assert!(p.strips.is_empty());
    let trace = p.take_trace();
    assert!(
        last_about(&trace, "a1b2c3").ends_with("a1b2c3 scrolled away; pictures deleted"),
        "{trace:#?}"
    );
}

#[test]
fn precut_a_card_scrolled_off_the_top_says_so_in_the_trace() {
    let mut p = painter(Route::KittyPrecut).with_trace();
    let _ = p.feed(BLOCK);
    // The harness writes a line at the foot of the screen three times, each
    // scrolling the card up a row, until none of its rows is left.
    for _ in 0..3 {
        let _ = p.feed(b"\x1b[10;1H\r\n");
    }
    assert!(p.strips.is_empty());
    let trace = p.take_trace();
    assert!(
        last_about(&trace, "a1b2c3").ends_with("a1b2c3 scrolled away; pictures deleted"),
        "{trace:#?}"
    );
}

#[test]
fn precut_sessions_number_their_pictures_apart() {
    let first = |session: u64| {
        let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default()).with_session(session);
        let _ = p.feed(BLOCK);
        p.strips["a1b2c3"][0].image
    };
    let (a, b) = (first(0x1111), first(0x1112));
    assert_ne!(a, b);
    for id in [a, b] {
        assert!(id >= SESSION_IMAGES && id % 2 == 0, "{id:#x}");
    }
    assert_eq!(a, first(0x1111), "the same session, the same ids");
    // A session's run of ids, thousands long, never meets its neighbour's.
    let run = |session: u64| {
        let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default()).with_session(session);
        (0..5000)
            .map(|_| p.take_image_id())
            .collect::<HashSet<u32>>()
    };
    assert!(run(0x1111).is_disjoint(&run(0x1112)));
    // The ids wrap within the session range.
    let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default()).with_session(1);
    p.next_image = LAST_SESSION_IMAGE;
    assert_eq!(p.take_image_id(), LAST_SESSION_IMAGE);
    assert_eq!(p.take_image_id(), SESSION_IMAGES);
    // In kitty the painter's ids are as before.
    let p = Painter::new(10, 40, Route::KittyPlaceholders, Fake::default()).with_session(0x1111);
    assert_eq!(p.next_image, FIRST_IMAGE);
}

// The entrance on the pre-cut route: frames sent ahead, one placed per
// tick by the time passed.

/// A pre-cut painter playing entrances, with the card's `frames - 1`
/// entrance frames sent ahead at 50 ms as images [`FIRST_IMAGE`] on.
fn entering_with(fake: Fake) -> Painter<Fake> {
    let mut p = Painter::new(10, 40, Route::KittyPrecut, fake).with_motion(None);
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    p
}

fn entering(frames: u16) -> Painter<Fake> {
    entering_with(Fake {
        frames,
        ..Fake::default()
    })
}

/// Entrance frame `n`'s image id, from frame 1: frame 0, the card at no
/// opacity, is never sent.
fn frame_image(n: u32) -> u32 {
    FIRST_IMAGE + 2 * (n - 1)
}

/// Whether `paint` places image `image`, sent ahead, over `rows` rows of
/// 20 columns, at any stacking order.
fn places(paint: &[u8], image: u32, rows: u16) -> bool {
    contains(
        paint,
        format!("\x1b_Ga=p,q=2,C=1,i={image},p=1,c=20,r={rows},z=").as_bytes(),
    )
}

/// Whether `paint` sends `png` as image `image` over `cols` by `rows`
/// cells, at any stacking order.
fn shows(paint: &[u8], image: u32, png: &[u8], cols: u16, rows: u16) -> bool {
    (1..500).any(|z| {
        contains(
            paint,
            &kitty::show_png_over(image, png, cols, rows, None, z),
        )
    })
}

/// The image ids `paint` deletes, in order.
fn deleted(paint: &[u8]) -> Vec<u32> {
    const KEY: &[u8] = b"\x1b_Ga=d,d=I,q=2,i=";
    paint
        .windows(KEY.len())
        .enumerate()
        .filter(|(_, w)| *w == KEY)
        .map(|(at, _)| {
            let digits: String = paint[at + KEY.len()..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .map(|b| char::from(*b))
                .collect();
            digits.parse().unwrap()
        })
        .collect()
}

/// The bytes that blank screen row `y` from column 0 over `width` columns
/// on the terminal's default background.
fn blank_row(y: u16, width: usize) -> Vec<u8> {
    [
        format!("\x1b[{};1H\x1b[0m", y + 1).into_bytes(),
        vec![b' '; width],
    ]
    .concat()
}

#[test]
fn precut_sends_the_entrance_ahead_as_an_image_per_frame() {
    let mut p = Painter::new(
        10,
        40,
        Route::KittyPrecut,
        Fake {
            frames: 4,
            ..Fake::default()
        },
    )
    .with_motion(None);
    p.pictures.fresh = vec!["a1b2c3".into()];
    let ahead = p.tick(Duration::from_millis(50));
    assert_eq!(
        ahead,
        [
            kitty::transmit_png(frame_image(1), b"frame 1"),
            kitty::transmit_png(frame_image(2), b"frame 2"),
        ]
        .concat(),
        "no placement, not frame 0, and not the half-resolution settled card"
    );
    assert!(p.tick(Duration::from_millis(100)).is_empty(), "sent once");
}

#[test]
fn precut_places_a_frame_per_tick_then_the_crisp_card() {
    let mut p = entering(4).with_trace();
    let paint = added(BLOCK, &p.feed(BLOCK));
    // Frame 0 is the card at no opacity: the first showing places frame 1.
    assert!(places(&paint, frame_image(1), 3));
    assert!(
        !contains(&paint, b"a=t") && !contains(&paint, b"a=T"),
        "sent ahead"
    );
    // Shown at 50 ms as if begun at 0 ms: frame 2 at 100 ms, the crisp card
    // at 150 ms, each over the one it replaces, which goes a tick later.
    assert!(!contains(&p.tick(Duration::from_millis(80)), b"\x1b_G"));
    let paint = p.tick(Duration::from_millis(100));
    assert!(places(&paint, frame_image(2), 3));
    assert!(deleted(&paint).is_empty(), "frame 1 stays under frame 2");
    let paint = p.tick(Duration::from_millis(150));
    let crisp = frame_image(3);
    assert!(shows(&paint, crisp, PNG, 20, 3));
    assert_eq!(
        deleted(&paint),
        [frame_image(1)],
        "frame 1, replaced a tick ago; frame 0 was never sent"
    );
    assert_eq!(
        p.tick(Duration::from_millis(200)),
        kitty::delete(frame_image(2))
    );
    assert!(p.tick(Duration::from_millis(250)).is_empty(), "settled");
    assert_eq!(p.feed(b"x"), b"x", "and stays so");
    let trace = p.take_trace();
    assert!(trace.iter().any(|l| l.contains("a1b2c3 entrance starts")));
    assert!(trace.iter().any(|l| l.contains("a1b2c3 entrance over")));
}

#[test]
fn precut_a_late_tick_skips_to_the_frame_for_the_time_passed() {
    let mut p = entering(6);
    let _ = p.feed(BLOCK);
    let _ = p.tick(Duration::from_millis(100));
    // Five frames, begun at 0 ms; at 210 ms it is frame 4's turn.
    let paint = p.tick(Duration::from_millis(210));
    assert!(places(&paint, frame_image(4), 3));
    assert!(!contains(
        &paint,
        format!("i={},", frame_image(3)).as_bytes()
    ));
    // Past the end: the crisp card, and the frame never placed is freed.
    let paint = p.tick(Duration::from_millis(250));
    assert!(shows(&paint, frame_image(5), PNG, 20, 3));
    assert!(contains(&paint, &kitty::delete(frame_image(3))));
    assert!(p.reels.is_empty());
}

#[test]
fn precut_a_card_moved_mid_entrance_keeps_its_frame_and_never_restarts() {
    let mut p = entering(6);
    let chunk = [b"\r\n".as_slice(), BLOCK].concat();
    let _ = p.feed(&chunk);
    let _ = p.tick(Duration::from_millis(150));
    // Frame 3 is on screen; the harness scrolls the block up a row.
    let paint = added(b"\x1b[10;1H\r\n", &p.feed(b"\x1b[10;1H\r\n"));
    assert_eq!(
        p.pictures.frame_bands.last(),
        Some(&(3, 0, 3)),
        "frame 3 again, cut fresh: a picture placed once is never placed again"
    );
    assert!(
        !contains(&paint, b"a=p,"),
        "no frame sent ahead is placed again"
    );
    let strip = p.strips["a1b2c3"][0];
    assert_eq!((strip.y, strip.frame), (0, Some(3)));
    assert!(contains(&paint, format!("i={},", strip.image).as_bytes()));
    assert!(!contains(&paint, &kitty::delete(frame_image(3))));
    // The next tick goes on from frame 4, sent ahead and never placed, and
    // frame 3 goes.
    let paint = p.tick(Duration::from_millis(200));
    assert!(places(&paint, frame_image(4), 3));
    assert!(contains(&paint, &kitty::delete(frame_image(3))));
    // Once over, a resize places the crisp card, not the entrance again.
    let _ = p.tick(Duration::from_millis(400));
    let paint = p.resize(10, 40);
    let paint = [paint, p.feed(b" ")].concat();
    assert!(contains(&paint, b"a=T"));
    assert!(!contains(&paint, b"a=p,"));
    assert_eq!(p.strips["a1b2c3"][0].frame, None);
}

/// The entrance frames trace lines say were placed, in order.
fn frames_placed(trace: &[String]) -> Vec<u16> {
    trace
        .iter()
        .filter_map(|l| l.split(" placed frame ").nth(1))
        .filter_map(|rest| rest.split(' ').next()?.parse().ok())
        .collect()
}

/// A 38-row dashboard whose frame bands are over a tick's budget, its 12
/// entrance frames sent ahead, its block typed whole at screen row 1 of 50
/// at 50 ms, the entrance begun at 0 ms.
fn dashboard_entering() -> Painter<Fake> {
    let fake = Fake {
        frames: 14,
        rows: 38,
        huge: true,
        ..Fake::default()
    };
    let mut p = Painter::new(50, 40, Route::KittyPrecut, fake)
        .with_motion(None)
        .with_trace();
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    let mut chunk = String::from("\r\n");
    for row in 1..=38u16 {
        let _ = write!(chunk, "[tile:a1b2c3 {row:02}/38]\r\n");
    }
    let _ = p.feed(chunk.as_bytes());
    p
}

#[test]
fn precut_a_dashboard_moved_mid_entrance_goes_on_to_its_next_frame() {
    let mut p = dashboard_entering();
    for t in [100, 150, 200] {
        let _ = p.tick(Duration::from_millis(t));
    }
    assert_eq!(frames_placed(&p.take_trace()), [1, 2, 3, 4]);
    // Frame 4 is on screen; the harness scrolls the card up a row. Frame 4's
    // band is over the tick's budget and a picture placed once is never
    // placed again, so frame 5, sent ahead, goes up a tick early.
    let paint = added(b"\x1b[50;1H\r\n", &p.feed(b"\x1b[50;1H\r\n"));
    assert!(places(&paint, frame_image(5), 38));
    assert_eq!(p.strips["a1b2c3"][0].y, 0);
    let mut placed = frames_placed(&p.take_trace());
    for t in (250..=650).step_by(50) {
        let paint = p.tick(Duration::from_millis(t));
        if t == 250 {
            assert!(!contains(&paint, b"a=p,"), "frame 5's turn: it is up");
        }
    }
    let trace = p.take_trace();
    placed.extend(frames_placed(&trace));
    assert_eq!(placed, [5, 6, 7, 8, 9, 10, 11, 12], "{trace:#?}");
    assert!(
        !trace.iter().any(|l| l.contains("entrance dropped")),
        "{trace:#?}"
    );
    assert_eq!(p.strips["a1b2c3"][0].frame, None, "the crisp card ends it");
}

#[test]
fn precut_a_dashboard_moved_on_its_last_frame_names_the_frame_it_could_not_send() {
    let mut p = dashboard_entering();
    let _ = p.tick(Duration::from_millis(600));
    assert_eq!(p.strips["a1b2c3"][0].frame, Some(12));
    let _ = p.feed(b"\x1b[50;1H\r\n");
    assert_eq!(p.strips["a1b2c3"][0].frame, None);
    let trace = p.take_trace();
    assert!(
        trace.iter().any(|l| l.ends_with(
            "a1b2c3 shown settled; entrance dropped: frame 12 is 71680 bytes, 0 frames unsent"
        )),
        "{trace:#?}"
    );
}

#[test]
fn precut_cards_enter_on_their_own_clocks_a_tick_apart() {
    let mut p = entering(4);
    p.pictures.fresh = vec![SECOND.into()];
    let _ = p.tick(Duration::from_millis(50));
    let _ = p.feed(BLOCK);
    p.set_now(Duration::from_millis(100));
    let second =
        format!("[tile:{SECOND} 01/03]\r\n[tile:{SECOND} 02/03]\r\n[tile:{SECOND} 03/03]\r\n");
    let _ = p.feed(second.as_bytes());
    let frame = |p: &Painter<Fake>, id: &str| p.strips[id][0].frame;
    assert_eq!((frame(&p, "a1b2c3"), frame(&p, SECOND)), (Some(2), Some(1)));
    let _ = p.tick(Duration::from_millis(150));
    assert_eq!((frame(&p, "a1b2c3"), frame(&p, SECOND)), (None, Some(2)));
    let _ = p.tick(Duration::from_millis(200));
    assert_eq!((frame(&p, "a1b2c3"), frame(&p, SECOND)), (None, None));
}

#[test]
fn precut_the_entrance_plays_on_the_whole_card_from_its_first_typed_row() {
    let mut p = entering(4);
    let chunk = b"[tile:a1b2c3 01/03] Signups\r\n";
    let paint = added(chunk, &p.feed(chunk));
    assert!(
        places(&paint, frame_image(1), 3),
        "the whole card's first frame, from the block's first row"
    );
    assert!(p.pictures.frame_bands.is_empty(), "the frame sent ahead");
    let chunk = b"[tile:a1b2c3 02/03]\r\n";
    let paint = added(chunk, &p.feed(chunk));
    assert!(!contains(&paint, b"\x1b_G"), "row 2 changes nothing");
    let paint = p.tick(Duration::from_millis(100));
    assert!(places(&paint, frame_image(2), 3));
    let chunk = b"[tile:a1b2c3 03/03]\r\n";
    let paint = added(chunk, &p.feed(chunk));
    assert!(!contains(&paint, b"\x1b_G"), "nor does row 3");
    assert!(p.whole.contains("a1b2c3"));
    let paint = p.tick(Duration::from_millis(150));
    let crisp = p.strips["a1b2c3"][0];
    assert_eq!((p.strips["a1b2c3"].len(), crisp.frame), (1, None));
    assert!(shows(&paint, crisp.image, PNG, 20, 3));
}

#[test]
fn precut_a_card_shown_before_its_entrance_arrives_stays_settled() {
    let mut p = Painter::new(
        10,
        40,
        Route::KittyPrecut,
        Fake {
            frames: 4,
            ..Fake::default()
        },
    )
    .with_motion(None);
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(contains(
        &paint,
        &kitty::show_png_over(FIRST_IMAGE, PNG, 20, 3, None, 1)
    ));
    p.pictures.fresh = vec!["a1b2c3".into()];
    assert!(p.tick(Duration::from_millis(50)).is_empty(), "never sent");
    assert!(p.reels.is_empty());
}

#[test]
fn precut_frames_sent_each_tick_and_too_large_ones_drop_the_entrance() {
    let mut p = Painter::new(
        10,
        40,
        Route::KittyPrecut,
        Fake {
            frames: 4,
            ..Fake::default()
        },
    )
    .with_motion(None)
    .with_frames_each_tick();
    p.pictures.fresh = vec!["a1b2c3".into()];
    assert!(
        p.tick(Duration::from_millis(50)).is_empty(),
        "nothing ahead"
    );
    let paint = added(BLOCK, &p.feed(BLOCK));
    let id = p.strips["a1b2c3"][0].image;
    assert!(contains(
        &paint,
        &kitty::show_png_over(id, b"frame 1 band 0 3", 20, 3, None, 1)
    ));
    let paint = p.tick(Duration::from_millis(100));
    let next = p.strips["a1b2c3"][0].image;
    assert!(contains(
        &paint,
        &kitty::show_png_over(next, b"frame 2 band 0 3", 20, 3, None, 2)
    ));
    assert!(!contains(&paint, &kitty::delete(id)), "a tick later");
    // A dashboard's frame is over the tick's budget: the crisp card.
    let mut p = Painter::new(
        10,
        40,
        Route::KittyPrecut,
        Fake {
            frames: 4,
            huge: true,
            ..Fake::default()
        },
    )
    .with_motion(None)
    .with_frames_each_tick()
    .with_trace();
    p.pictures.fresh = vec!["a1b2c3".into()];
    let _ = p.tick(Duration::from_millis(50));
    let paint = added(BLOCK, &p.feed(BLOCK));
    assert!(contains(
        &paint,
        &kitty::show_png_over(FIRST_IMAGE + 4, PNG, 20, 3, None, 1)
    ));
    assert!(p.reels.is_empty());
    assert_eq!(p.strips["a1b2c3"][0].frame, None);
    let trace = p.take_trace();
    assert!(
        trace
            .iter()
            .any(|l| l.contains("entrance dropped: frame 1 is 71680 bytes")),
        "{trace:#?}"
    );
}

#[test]
fn a_block_whose_read_ends_mid_sequence_is_painted_in_that_read() {
    // The harness wrote the block in one go; the read ended inside the SGR
    // that starts the next line.
    for route in [Route::KittyPrecut, Route::KittyPlaceholders] {
        let mut p = painter(route);
        let chunk = [BLOCK, b"\x1b[3".as_slice()].concat();
        let out = p.feed(&chunk);
        assert!(out.starts_with(SYNC_ON), "{route:?}");
        let body = &out[SYNC_ON.len()..];
        assert!(
            body.starts_with(BLOCK),
            "{route:?}: the harness's bytes first"
        );
        assert!(
            contains(body, b"\x1b_G"),
            "{route:?}: the card is drawn now"
        );
        assert!(
            body.ends_with(b"\x1b[?2026l\x1b[3"),
            "{route:?}: the unfinished sequence follows the paint's update"
        );
        let rest = p.feed(b"1mok");
        assert_eq!(rest, b"1mok", "{route:?}: nothing painted twice");
    }
}

#[test]
fn a_read_that_begins_mid_sequence_paints_its_rows_in_the_update() {
    // The previous read ended inside the SGR that starts the second row;
    // this read begins with that SGR's end, then two typed rows. The
    // sequence's end passes before the paint's update, the rows go in it.
    for route in [Route::KittyPrecut, Route::KittyPlaceholders] {
        let mut p = painter(route);
        let first = b"[tile:a1b2c3 01/03] Signups dipped\r\n\x1b[48;2;10;10";
        let first_out = p.feed(first);
        assert!(
            first_out.ends_with(b"\x1b[?2026l\x1b[48;2;10;10"),
            "{route:?}: the unfinished sequence follows the paint's update"
        );
        let rows = b"[tile:a1b2c3 02/03]\r\n[tile:a1b2c3 03/03]\r\n";
        let out = p.feed(&[b";10m".as_slice(), rows].concat());
        let expected = [b";10m".as_slice(), SYNC_ON, rows].concat();
        assert!(
            out.starts_with(&expected),
            "{route:?}: the rows follow the update's start, not the sequence's end: {:?}",
            String::from_utf8_lossy(&out)
        );
        // On the pre-cut route the whole card went up with the first row,
        // and the rows typed under it are blanked.
        let drawn = match route {
            Route::KittyPrecut => contains(&first_out, b"\x1b_G") && contains(&out, &[b' '; 20]),
            _ => contains(&out, b"\x1b_G"),
        };
        assert!(drawn, "{route:?}: the rows are drawn");
        assert!(out.ends_with(b"\x1b[?2026l"), "{route:?}");
    }
}

#[test]
fn precut_pi_streamed_closing_fence_is_blanked_left_of_a_growing_card() {
    // Pi streams a fenced block with its closing fence under the last row
    // typed so far, two columns left of the card's.
    let mut p = Painter::new(10, 60, Route::KittyPrecut, Fake::default());
    let mut term = vt100::Parser::new(10, 60, 0);
    for (typed, chunk) in [
        b"```tiles\r\n  [tile:a1b2c3 01/03] Signups\r\n```".as_slice(),
        b"\r  [tile:a1b2c3 02/03]\x1b[K\r\n```",
        b"\r  [tile:a1b2c3 03/03]\x1b[K\r\n```",
    ]
    .into_iter()
    .enumerate()
    {
        term.process(&p.feed(chunk));
        let rows: Vec<String> = term
            .screen()
            .rows(0, 60)
            .map(|r| r.trim_end().to_string())
            .collect();
        assert!(
            rows.iter().all(|r| !r.contains('`')),
            "row {} typed: {rows:?}",
            typed + 1
        );
    }
    assert!(p.whole.contains("a1b2c3"));
    assert_eq!(p.strips["a1b2c3"].len(), 1, "one picture all along");
}

#[test]
fn precut_a_resize_keeps_the_card_until_it_is_placed_again() {
    let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default()).with_trace();
    let chunk = [b"\x1b[5;1H".as_slice(), BLOCK].concat();
    let _ = p.feed(&chunk);
    let image = p.strips["a1b2c3"][0].image;
    // The terminal loses three rows; the harness has not drawn yet.
    p.set_now(Duration::from_millis(100));
    let out = p.resize(7, 40);
    assert!(deleted(&out).is_empty(), "no picture deleted on the resize");
    for t in [150, 200, 250, 300] {
        let out = p.tick(Duration::from_millis(t));
        assert!(deleted(&out).is_empty(), "{t} ms: a quiet harness keeps it");
    }
    assert_eq!(p.strips["a1b2c3"][0].image, image);
    // The harness draws its screen again, the block three rows up: the card
    // goes up there, over the old picture, which goes a tick later.
    p.set_now(Duration::from_millis(320));
    let chunk = [b"\x1b[H\x1b[2J\x1b[2;1H".as_slice(), BLOCK].concat();
    let paint = added(&chunk, &p.feed(&chunk));
    assert_eq!(rows_placed(&paint), [3]);
    assert!(!deleted(&paint).contains(&image), "not in the same update");
    let strip = p.strips["a1b2c3"][0];
    assert_eq!((strip.y, strip.lo), (1, 1));
    assert_ne!(strip.image, image);
    let out = p.tick(Duration::from_millis(400));
    assert_eq!(deleted(&out), [image], "the old picture goes a tick later");
    let trace = p.take_trace();
    assert!(
        trace
            .iter()
            .any(|l| l.contains("resized to 7 by 40; 1 cards kept until placed again")),
        "{trace:#?}"
    );
}

#[test]
fn precut_a_resize_places_the_card_again_where_its_block_stands() {
    // The harness draws the block on the same row after the resize, but
    // the terminal may have moved the picture with its text: placed again.
    let mut p = Painter::new(10, 40, Route::KittyPrecut, Fake::default());
    let _ = p.feed(BLOCK);
    let image = p.strips["a1b2c3"][0].image;
    let _ = p.resize(12, 40);
    p.set_now(Duration::from_millis(100));
    let chunk = [b"\x1b[H\x1b[2J".as_slice(), BLOCK].concat();
    let paint = added(&chunk, &p.feed(&chunk));
    assert_eq!(rows_placed(&paint), [3]);
    assert_ne!(p.strips["a1b2c3"][0].image, image);
    assert!(p.resized.is_empty());
    // Typed on, it stays put.
    p.set_now(Duration::from_millis(200));
    let paint = added(b"more", &p.feed(b"more"));
    assert_eq!(rows_placed(&paint), [] as [u16; 0]);
}

#[test]
fn precut_a_block_waiting_for_room_shows_no_title() {
    let mut p = waiting_for_room();
    p.pictures.cols = 36;
    let mut term = vt100::Parser::new(30, 40, 0);
    term.process(&b"\r\n".repeat(29));
    for row in 1..=3u16 {
        p.set_now(Duration::from_millis(100 + u64::from(row) * 120));
        let mut chunk = format!("\x1b[26;1H\x1b[J[tile:a1b2c3 {row:02}/23]");
        if row == 1 {
            chunk.push_str(" Weekly signups");
        }
        for line in ["+------+", "| > ask", "|", "|", "+------+"] {
            chunk.push_str("\r\n");
            chunk.push_str(line);
        }
        term.process(&p.feed(chunk.as_bytes()));
        let screen = term.screen().contents();
        assert!(!screen.contains("[tile"), "row {row}: {screen}");
        assert!(!screen.contains("Weekly"), "row {row}: {screen}");
        assert!(screen.contains("| > ask"), "row {row}: the input box stays");
    }
    assert!(p.strips.is_empty(), "the block still waits");
}

#[test]
fn precut_a_waiting_block_keeps_the_text_right_of_its_card() {
    // A side panel's border at column 30 on every row of a block waiting
    // for room: right of a 30-column card, and inside a 36-column one,
    // which it cuts to 30. The harness redraws only its input box, so a
    // cell the painter blanks stays blank unless it is written back. Only
    // the rows under the first cut the card, and the harness scrolls the
    // first row up without writing it again when it comes alone.
    for (cols, alone) in [(30, false), (36, false), (36, true)] {
        let mut p = waiting_for_room();
        p.pictures.cols = cols;
        let mut term = vt100::Parser::new(30, 40, 0);
        let mut harness = vt100::Parser::new(30, 40, 0);
        term.process(&b"\r\n".repeat(29));
        harness.process(&b"\r\n".repeat(29));
        let mut rows = (if alone { 1 } else { 2 })..=23u16;
        for t in (100..=3720u64).step_by(10) {
            let now = Duration::from_millis(t);
            if t % 50 == 0 {
                term.process(&p.tick(now));
            }
            if t % 120 == 0
                && let Some(row) = rows.next()
            {
                p.set_now(now);
                let first = if row == 2 && !alone {
                    "[tile:a1b2c3 01/23] Signups\x1b[31G\u{2502}\r\n"
                } else {
                    ""
                };
                let title = if row == 1 { " Signups" } else { "" };
                let mut chunk = format!(
                    "\x1b[26;1H\x1b[J{first}[tile:a1b2c3 {row:02}/23]{title}\x1b[31G\u{2502}"
                );
                for line in ["+------+", "| > ask", "|", "|", "+------+"] {
                    chunk.push_str("\r\n");
                    chunk.push_str(line);
                }
                term.process(&p.feed(chunk.as_bytes()));
                harness.process(chunk.as_bytes());
            }
            let screen = term.screen().contents();
            assert!(!screen.contains("Signups"), "{cols} cols, {t} ms: {screen}");
            assert_eq!(
                screen.matches('\u{2502}').count(),
                harness.screen().contents().matches('\u{2502}').count(),
                "{cols} cols, alone {alone}, {t} ms: every border stays: {screen}"
            );
        }
        assert!(rows.next().is_none(), "23 rows typed");
        assert!(!p.strips.is_empty(), "{cols} cols: the card went up");
    }
}
