//! The kitty graphics protocol, the one encoder for it
//! (<https://sw.kovidgoyal.net/kitty/graphics-protocol/>).
//!
//! Every command this module writes is `ESC _ G <keys> ; <base64> ESC \`, where
//! the keys are numbers and letters this module chose and the payload is
//! base64, so nothing from a chart's data can reach the terminal through it
//! (ADR-009). Responses are suppressed (`q=2`) so a terminal's `OK` never lands
//! in the harness's input.

use std::fmt::Write as _;

use crate::b64;
use crate::diacritics::DIACRITICS;

/// Base64 bytes per chunk; the protocol's limit is 4096.
pub const CHUNK: usize = 4096;
/// The Unicode placeholder character a virtual placement is drawn through.
pub const PLACEHOLDER: char = '\u{10EEEE}';
/// Most rows or columns a placeholder can number.
pub const MAX_CELLS: u16 = 297;

/// A part of the image, in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

fn command(keys: &str, payload: &[u8]) -> Vec<u8> {
    let data = b64::encode(payload);
    let mut out = Vec::with_capacity(data.len() + data.len() / CHUNK * 16 + keys.len() + 16);
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.as_bytes().chunks(CHUNK).collect()
    };
    let last = chunks.len() - 1;
    for (i, chunk) in chunks.into_iter().enumerate() {
        out.extend_from_slice(b"\x1b_G");
        if i == 0 {
            out.extend_from_slice(keys.as_bytes());
            out.push(b',');
        }
        out.extend_from_slice(if i == last { b"m=0" } else { b"m=1" });
        out.push(b';');
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

/// Sends a PNG and places it at the cursor over `cols` by `rows` cells,
/// leaving the cursor where it was (`C=1`). `crop` sends only that part,
/// for terminals that honour crop rectangles.
#[must_use]
pub fn show_png(id: u32, png: &[u8], cols: u16, rows: u16, crop: Option<Crop>) -> Vec<u8> {
    let mut keys = format!("a=T,f=100,t=d,q=2,C=1,i={id},c={cols},r={rows}");
    if let Some(c) = crop {
        let _ = write!(keys, ",x={},y={},w={},h={}", c.x, c.y, c.w, c.h);
    }
    command(&keys, png)
}

/// [`show_png`] at stacking order `z` (above the text when 0 or more), so a
/// picture placed over an older one it replaces draws on top of it.
#[must_use]
pub fn show_png_over(
    id: u32,
    png: &[u8],
    cols: u16,
    rows: u16,
    crop: Option<Crop>,
    z: i32,
) -> Vec<u8> {
    let mut keys = format!("a=T,f=100,t=d,q=2,C=1,i={id},c={cols},r={rows},z={z}");
    if let Some(c) = crop {
        let _ = write!(keys, ",x={},y={},w={},h={}", c.x, c.y, c.w, c.h);
    }
    command(&keys, png)
}

/// Sends a PNG with a virtual placement of `cols` by `rows` cells, drawn
/// wherever [`placeholder_rows`] cells for `id` appear (`U=1`).
#[must_use]
pub fn virtual_png(id: u32, png: &[u8], cols: u16, rows: u16) -> Vec<u8> {
    command(
        &format!("a=T,f=100,t=d,q=2,U=1,i={id},c={cols},r={rows}"),
        png,
    )
}

/// Sends a PNG as image `id` without placing it (`a=t`): the first frame of
/// an animation, or a picture placed later.
#[must_use]
pub fn transmit_png(id: u32, png: &[u8]) -> Vec<u8> {
    command(&format!("a=t,f=100,t=d,q=2,i={id}"), png)
}

/// Adds a PNG as the next frame of image `id`'s animation (`a=f`), shown for
/// `gap_ms` before the frame after it.
#[must_use]
pub fn frame_png(id: u32, png: &[u8], gap_ms: u32) -> Vec<u8> {
    command(&format!("a=f,f=100,t=d,q=2,i={id},z={gap_ms}"), png)
}

/// A virtual placement of image `id` over `cols` by `rows` cells (`U=1`),
/// drawn wherever [`placeholder_rows`] cells for `id` appear.
#[must_use]
pub fn virtual_place(id: u32, cols: u16, rows: u16) -> Vec<u8> {
    format!("\x1b_Ga=p,U=1,q=2,i={id},c={cols},r={rows}\x1b\\").into_bytes()
}

/// Places image `id` at the cursor as placement `placement`, over `cols` by
/// `rows` cells, at stacking order `z` (above the text when 0 or more), and
/// leaves the cursor where it was. `crop` shows only that part of the image.
/// Placing the same `id` and `placement` again moves it.
#[must_use]
pub fn place(id: u32, placement: u32, cols: u16, rows: u16, z: i32, crop: Option<Crop>) -> Vec<u8> {
    let mut keys = format!("a=p,q=2,C=1,i={id},p={placement},c={cols},r={rows},z={z}");
    if let Some(c) = crop {
        let _ = write!(keys, ",x={},y={},w={},h={}", c.x, c.y, c.w, c.h);
    }
    format!("\x1b_G{keys}\x1b\\").into_bytes()
}

/// Removes placement `placement` of image `id`, keeping the image's data.
#[must_use]
pub fn delete_placement(id: u32, placement: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,q=2,i={id},p={placement}\x1b\\").into_bytes()
}

/// Makes frame `frame` (from 1) the one image `id` shows, and stops its
/// animation there.
#[must_use]
pub fn show_frame(id: u32, frame: u32) -> Vec<u8> {
    format!("\x1b_Ga=a,q=2,i={id},c={frame},s=1\x1b\\").into_bytes()
}

/// Deletes image `id` and frees its data, removing every placement of it.
#[must_use]
pub fn delete(id: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=I,q=2,i={id}\x1b\\").into_bytes()
}

/// The query that asks whether the terminal speaks the protocol: a one-pixel
/// image with id 31 sent for a reply only (`a=q`).
pub const QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

/// The text of the placeholder cells for image `id`: `rows` lines of `cols`
/// cells, each line coloured with the id (24-bit foreground, the protocol's
/// encoding of ids above 255) and reset at its end. Every cell carries its
/// row and column marks, so a terminal that ignores the marks' inheritance
/// still finds each cell's place.
///
/// `cols` and `rows` are clamped to [`MAX_CELLS`].
#[must_use]
pub fn placeholder_rows(id: u32, cols: u16, rows: u16) -> Vec<String> {
    let (r, g, b) = ((id >> 16) & 0xff, (id >> 8) & 0xff, id & 0xff);
    let cols = usize::from(cols.min(MAX_CELLS));
    let rows = usize::from(rows.min(MAX_CELLS));
    (0..rows)
        .map(|row| {
            let mut line = format!("\x1b[38;2;{r};{g};{b}m");
            for col_mark in &DIACRITICS[..cols] {
                line.push(PLACEHOLDER);
                line.push(DIACRITICS[row]);
                line.push(*col_mark);
            }
            line.push_str("\x1b[39m");
            line
        })
        .collect()
}

/// An image id for a card: never 0, and with all three colour bytes used,
/// so the 24-bit foreground encoding is unambiguous.
#[must_use]
pub fn image_id(seed: u64) -> u32 {
    let folded = u32::try_from((seed ^ (seed >> 24) ^ (seed >> 48)) & 0x00ff_ffff).unwrap_or(0);
    folded | 0x0001_0101
}

#[cfg(test)]
mod tests {
    use super::{
        CHUNK, Crop, MAX_CELLS, PLACEHOLDER, delete, delete_placement, frame_png, image_id, place,
        placeholder_rows, show_frame, show_png, show_png_over, transmit_png, virtual_place,
        virtual_png,
    };
    use crate::diacritics::DIACRITICS;

    fn commands(bytes: &[u8]) -> Vec<String> {
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        text.split("\x1b\\")
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.strip_prefix("\x1b_G")
                    .expect("each command starts with APC G")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn a_small_png_is_one_command_with_its_keys() {
        let out = commands(&show_png(7, b"\x89PNG", 40, 12, None));
        assert_eq!(out, ["a=T,f=100,t=d,q=2,C=1,i=7,c=40,r=12,m=0;iVBORw=="]);
    }

    #[test]
    fn a_large_png_is_chunked_with_keys_only_on_the_first() {
        let png = vec![0u8; CHUNK * 2];
        let out = commands(&virtual_png(9, &png, 10, 5));
        assert!(out.len() >= 3);
        assert!(out[0].starts_with("a=T,f=100,t=d,q=2,U=1,i=9,c=10,r=5,m=1;"));
        for middle in &out[1..out.len() - 1] {
            assert!(middle.starts_with("m=1;"));
        }
        assert!(out.last().unwrap().starts_with("m=0;"));
        let payload: usize = out.iter().map(|c| c.split_once(';').unwrap().1.len()).sum();
        assert_eq!(payload, (CHUNK * 2).div_ceil(3) * 4);
        assert!(
            out.iter()
                .all(|c| c.split_once(';').unwrap().1.len() <= CHUNK)
        );
    }

    #[test]
    fn a_picture_over_another_names_its_order() {
        let out = commands(&show_png_over(7, b"\x89PNG", 40, 12, None, 3));
        assert_eq!(
            out,
            ["a=T,f=100,t=d,q=2,C=1,i=7,c=40,r=12,z=3,m=0;iVBORw=="]
        );
    }

    #[test]
    fn a_crop_names_its_rectangle() {
        let crop = Crop {
            x: 0,
            y: 40,
            w: 640,
            h: 200,
        };
        let out = commands(&show_png(3, b"x", 4, 2, Some(crop)));
        assert!(out[0].contains(",x=0,y=40,w=640,h=200,m=0;"));
    }

    #[test]
    fn an_animation_is_a_picture_its_frames_and_the_frame_shown() {
        assert_eq!(
            commands(&transmit_png(4, b"x")),
            ["a=t,f=100,t=d,q=2,i=4,m=0;eA=="]
        );
        assert_eq!(
            commands(&frame_png(4, b"x", 28)),
            ["a=f,f=100,t=d,q=2,i=4,z=28,m=0;eA=="]
        );
        assert_eq!(
            virtual_place(4, 40, 9),
            b"\x1b_Ga=p,U=1,q=2,i=4,c=40,r=9\x1b\\"
        );
        assert_eq!(show_frame(4, 17), b"\x1b_Ga=a,q=2,i=4,c=17,s=1\x1b\\");
    }

    #[test]
    fn a_placement_names_its_id_order_and_crop() {
        let crop = Crop {
            x: 0,
            y: 0,
            w: 640,
            h: 96,
        };
        assert_eq!(
            place(4, 2, 40, 3, 2, Some(crop)),
            b"\x1b_Ga=p,q=2,C=1,i=4,p=2,c=40,r=3,z=2,x=0,y=0,w=640,h=96\x1b\\"
        );
        assert_eq!(delete_placement(4, 2), b"\x1b_Ga=d,d=i,q=2,i=4,p=2\x1b\\");
    }

    #[test]
    fn delete_frees_the_data() {
        assert_eq!(delete(5), b"\x1b_Ga=d,d=I,q=2,i=5\x1b\\");
    }

    #[test]
    fn placeholder_cells_carry_the_id_and_both_marks() {
        let id = 0x0a_0b_0c;
        let rows = placeholder_rows(id, 3, 2);
        assert_eq!(rows.len(), 2);
        let mut expected = "\x1b[38;2;10;11;12m".to_string();
        for mark in &DIACRITICS[..3] {
            expected.extend([PLACEHOLDER, DIACRITICS[1], *mark]);
        }
        expected.push_str("\x1b[39m");
        assert_eq!(rows[1], expected);
    }

    #[test]
    fn placeholder_sizes_are_clamped_to_the_marks_available() {
        let rows = placeholder_rows(0x01_0101, 400, 400);
        assert_eq!(rows.len(), usize::from(MAX_CELLS));
        assert_eq!(
            rows[0].chars().filter(|&c| c == PLACEHOLDER).count(),
            usize::from(MAX_CELLS)
        );
    }

    #[test]
    fn max_cells_is_the_number_of_marks() {
        assert_eq!(usize::from(MAX_CELLS), DIACRITICS.len());
    }

    #[test]
    fn image_ids_are_never_zero_and_fit_24_bits() {
        for seed in [0, 1, u64::MAX, 0xdead_beef_cafe] {
            let id = image_id(seed);
            assert_ne!(id, 0);
            assert!(id <= 0x00ff_ffff);
            assert!(id & 0xff != 0 && (id >> 8) & 0xff != 0 && (id >> 16) != 0);
        }
    }
}
