//! Unpremultiplied RGBA pixels, and the cut a partly visible card needs.

/// An image as rows of unpremultiplied RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes per pixel, row by row.
    pub data: Vec<u8>,
}

impl Rgba {
    /// An image, when `data` holds exactly `width * height` pixels.
    #[must_use]
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (data.len() == expected).then_some(Self {
            width,
            height,
            data,
        })
    }

    /// The pixel at `x`, `y`; the caller keeps them in range.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// Rows `top` to `top + height` (clamped to the image), the part of a card
    /// a terminal that ignores crop rectangles is sent instead (ADR-004).
    #[must_use]
    pub fn rows(&self, top: u32, height: u32) -> Rgba {
        let top = top.min(self.height);
        let height = height.min(self.height - top);
        let stride = self.width as usize * 4;
        let start = top as usize * stride;
        Rgba {
            width: self.width,
            height,
            data: self.data[start..start + height as usize * stride].to_vec(),
        }
    }
}

/// A PNG's width and height from its header, when it is a PNG.
#[must_use]
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let header = png.get(..24)?;
    if !header.starts_with(b"\x89PNG\r\n\x1a\n") || &header[12..16] != b"IHDR" {
        return None;
    }
    let word = |at: usize| {
        u32::from_be_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    Some((word(16), word(20)))
}

#[cfg(test)]
mod tests {
    use super::{Rgba, png_size};

    #[test]
    fn the_size_comes_from_the_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&1344u32.to_be_bytes());
        png.extend_from_slice(&512u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1344, 512)));
        assert_eq!(png_size(&png[..23]), None);
        assert_eq!(png_size(b"not a png at all, not at all"), None);
    }

    fn ramp(w: u32, h: u32) -> Rgba {
        let data = (0..w * h)
            .flat_map(|i| [i.to_le_bytes()[0], 0, 0, 255])
            .collect();
        Rgba::new(w, h, data).unwrap()
    }

    #[test]
    fn new_refuses_a_buffer_of_the_wrong_size() {
        assert!(Rgba::new(2, 2, vec![0; 15]).is_none());
        assert!(Rgba::new(u32::MAX, u32::MAX, vec![]).is_none());
    }

    #[test]
    fn rows_cuts_whole_rows_and_clamps() {
        let img = ramp(3, 4);
        let cut = img.rows(1, 2);
        assert_eq!((cut.width, cut.height), (3, 2));
        assert_eq!(cut.pixel(0, 0), img.pixel(0, 1));
        assert_eq!(img.rows(3, 10).height, 1);
        assert_eq!(img.rows(9, 10).height, 0);
    }
}
