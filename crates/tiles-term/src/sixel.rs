//! Sixel, for terminals with sixel and no kitty graphics (ADR-004).
//!
//! The encoder maps each pixel to a fixed 6x6x6 colour cube plus a 24-step
//! grey ramp (240 registers), which keeps it deterministic, fast and free of
//! a quantizer dependency; card colours are flat, so the loss is in
//! anti-aliased edges only. Pixels under half opacity are left transparent.
//! The output holds only the DCS introducer, numbers, the sixel data
//! characters `?` to `~`, `#`, `!`, `$`, `-` and the ST terminator.

use std::fmt::Write as _;

use crate::image::Rgba;

const GREYS: u8 = 24;
const REGISTERS: usize = 216 + GREYS as usize;

fn level(v: u8) -> usize {
    (usize::from(v) * 5 + 127) / 255
}

/// The register of a colour.
fn register(r: u8, g: u8, b: u8) -> usize {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max - min < 10 {
        let grey = usize::from(max.midpoint(min));
        return 216 + (grey * usize::from(GREYS - 1) + 127) / 255;
    }
    level(r) * 36 + level(g) * 6 + level(b)
}

/// The colour of a register, as sixel percentages.
fn percent(reg: usize) -> (u32, u32, u32) {
    if reg >= 216 {
        let p = u32::try_from((reg - 216) * 100 / usize::from(GREYS - 1)).unwrap_or(100);
        return (p, p, p);
    }
    let pct = |l: usize| u32::try_from(l * 20).unwrap_or(100);
    (pct(reg / 36), pct(reg / 6 % 6), pct(reg % 6))
}

/// The image as one sixel picture.
#[must_use]
#[allow(clippy::many_single_char_names)] // w, h, r, g and b name the axes and channels.
pub fn encode(img: &Rgba) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let regs: Vec<Option<usize>> = (0..w * h)
        .map(|i| {
            let p = &img.data[i * 4..i * 4 + 4];
            (p[3] >= 128).then(|| register(p[0], p[1], p[2]))
        })
        .collect();
    let mut used = [false; REGISTERS];
    for r in regs.iter().flatten() {
        used[*r] = true;
    }
    let mut out = String::new();
    let _ = write!(out, "\x1bP0;1;0q\"1;1;{w};{h}");
    for (reg, _) in used.iter().enumerate().filter(|(_, u)| **u) {
        let (r, g, b) = percent(reg);
        let _ = write!(out, "#{reg};2;{r};{g};{b}");
    }
    for band in 0..h.div_ceil(6) {
        let mut present = [false; REGISTERS];
        for y in band * 6..(band * 6 + 6).min(h) {
            for r in regs[y * w..y * w + w].iter().flatten() {
                present[*r] = true;
            }
        }
        let mut first = true;
        for reg in (0..REGISTERS).filter(|r| present[*r]) {
            if !first {
                out.push('$');
            }
            first = false;
            let _ = write!(out, "#{reg}");
            let mut run: Option<(u8, usize)> = None;
            for x in 0..w {
                let mut bits = 0u8;
                for dy in 0..6 {
                    let y = band * 6 + dy;
                    if y < h && regs[y * w + x] == Some(reg) {
                        bits |= 1 << dy;
                    }
                }
                run = match run {
                    Some((b, n)) if b == bits => Some((b, n + 1)),
                    Some((b, n)) => {
                        push_run(&mut out, b, n);
                        Some((bits, 1))
                    }
                    None => Some((bits, 1)),
                };
            }
            if let Some((b, n)) = run
                && b != 0
            {
                push_run(&mut out, b, n);
            }
        }
        out.push('-');
    }
    out.push_str("\x1b\\");
    out.into_bytes()
}

fn push_run(out: &mut String, bits: u8, n: usize) {
    let c = char::from(63 + bits);
    if n > 3 {
        let _ = write!(out, "!{n}{c}");
    } else {
        for _ in 0..n {
            out.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{encode, percent, register};
    use crate::image::Rgba;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Rgba {
        Rgba::new(w, h, px.repeat((w * h) as usize)).unwrap()
    }

    #[test]
    fn a_solid_block_is_one_register_run_length_encoded() {
        let out = String::from_utf8(encode(&solid(10, 6, [255, 0, 0, 255]))).unwrap();
        assert_eq!(out, "\x1bP0;1;0q\"1;1;10;6#180;2;100;0;0#180!10~-\x1b\\");
    }

    #[test]
    fn partial_bands_and_transparency() {
        let mut img = solid(2, 7, [0, 0, 0, 255]);
        img.data[3] = 0;
        let out = String::from_utf8(encode(&img)).unwrap();
        assert!(out.starts_with("\x1bP0;1;0q\"1;1;2;7#216;2;0;0;0#216"));
        assert_eq!(out.matches('-').count(), 2);
        assert!(out.contains("#216}~-#216@@-"));
    }

    #[test]
    fn greys_use_the_ramp_and_colours_the_cube() {
        assert_eq!(register(128, 128, 128), 216 + 12);
        assert_eq!(register(0, 0, 255), 5);
        assert_eq!(percent(5), (0, 0, 100));
        assert_eq!(percent(239), (100, 100, 100));
    }

    #[test]
    fn output_is_only_sixel_syntax() {
        let data: Vec<u8> = (0..64u32 * 13).flat_map(u32::to_le_bytes).collect();
        let out = encode(&Rgba::new(64, 13, data).unwrap());
        let body = &out[2..out.len() - 2];
        assert!(body.iter().all(|&b| (0x20..0x7f).contains(&b)));
    }
}
