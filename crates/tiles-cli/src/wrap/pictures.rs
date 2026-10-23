//! Cards from the card cache, decoded and cut for the wrapper's routes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use tiles_term::image::Rgba;

use super::paint::{CardInfo, Pictures};

/// Largest card file the wrapper reads.
const MAX_CARD_BYTES: u64 = 16 * 1024 * 1024;

/// Most entrance frames the wrapper reads for one card.
const MAX_FRAMES: u16 = 60;

/// Cards read from `<cache>/cards/<id>.png` and `<id>.json`, and their
/// entrance frames from `<id>-anim-NN.png`.
pub struct CachePictures {
    cards: PathBuf,
    loaded: HashMap<String, Option<CardInfo>>,
    pixels: HashMap<String, Rc<Rgba>>,
    since: SystemTime,
    listed: Option<SystemTime>,
    /// Each card listed so far, with when its geometry was written.
    seen: HashMap<String, SystemTime>,
    /// This session's cards drawn again since the last call, such as in a
    /// new theme.
    redrawn: Vec<String>,
    session: u64,
}

impl CachePictures {
    /// Cards under `cache_dir`, drawing and sending ahead only those made
    /// for `session`.
    pub fn new(cache_dir: &std::path::Path, session: u64) -> Self {
        Self {
            cards: cache_dir.join("cards"),
            loaded: HashMap::new(),
            pixels: HashMap::new(),
            since: SystemTime::now(),
            listed: None,
            seen: HashMap::new(),
            redrawn: Vec::new(),
            session,
        }
    }

    /// Whether card `id`'s geometry names this session; `None` while it
    /// cannot be read.
    fn ours(&self, id: &str) -> Option<bool> {
        let meta: serde_json::Value =
            serde_json::from_slice(&self.read(&format!("{id}.json"))?).ok()?;
        Some(self.names_session(&meta))
    }

    /// Whether geometry `meta` names this session. The wrapper draws only
    /// such cards: a block naming any other, from an earlier session in
    /// scrollback, another `tiles run`, a file the harness shows or a model
    /// reply, stays text.
    fn names_session(&self, meta: &serde_json::Value) -> bool {
        meta.get("session")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| u64::from_str_radix(s, 16).ok())
            == Some(self.session)
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        std::fs::File::open(self.cards.join(name))
            .ok()?
            .take(MAX_CARD_BYTES)
            .read_to_end(&mut bytes)
            .ok()?;
        Some(bytes)
    }

    fn rgba(&mut self, id: &str) -> Option<Rc<Rgba>> {
        if let Some(p) = self.pixels.get(id) {
            return Some(p.clone());
        }
        let card = self.card(id)?;
        let decoded = Rc::new(decode(&card.png)?);
        if self.pixels.len() > 8 {
            self.pixels.clear();
        }
        self.pixels.insert(id.to_string(), decoded.clone());
        Some(decoded)
    }
}

/// Longest side of a card picture the wrapper decodes: the engine's widest
/// card (2400 CSS pixels) at the largest scale (4) fits with room to spare.
pub const MAX_SIDE: i32 = 16_384;
/// Most pixels the wrapper decodes, so a planted PNG whose header claims
/// 65535 by 65535 pixels is refused before 17 GB is allocated.
pub const MAX_PIXELS: i64 = 1 << 26;

/// PNG bytes as unpremultiplied RGBA, through Skia. `None` for a picture
/// larger than [`MAX_SIDE`] or [`MAX_PIXELS`].
pub fn decode(png: &[u8]) -> Option<Rgba> {
    let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(png))?;
    let (w, h) = (image.width(), image.height());
    if !(1..=MAX_SIDE).contains(&w)
        || !(1..=MAX_SIDE).contains(&h)
        || i64::from(w) * i64::from(h) > MAX_PIXELS
    {
        return None;
    }
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let row = usize::try_from(w).ok()? * 4;
    let mut data = vec![0u8; row * usize::try_from(h).ok()?];
    image
        .read_pixels(
            &info,
            &mut data,
            row,
            (0, 0),
            skia_safe::image::CachingHint::Disallow,
        )
        .then_some(())?;
    Rgba::new(u32::try_from(w).ok()?, u32::try_from(h).ok()?, data)
}

impl Pictures for CachePictures {
    fn card(&mut self, id: &str) -> Option<CardInfo> {
        if !tiles_term::block::is_card_id(id) {
            return None;
        }
        if let Some(c) = self.loaded.get(id) {
            return c.clone();
        }
        let meta: Option<serde_json::Value> = self
            .read(&format!("{id}.json"))
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        if let Some(meta) = &meta
            && !self.names_session(meta)
        {
            self.loaded.insert(id.to_string(), None);
            return None;
        }
        let info = (|| {
            let meta = meta?;
            let num = |k: &str| {
                meta.get(k)?
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|n| (1..=300).contains(n))
            };
            let png = self.read(&format!("{id}.png"))?;
            let size_px = tiles_term::image::png_size(&png)?;
            let small = |k: &str| {
                meta.get(k)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u16::try_from(n).ok())
                    .unwrap_or(0)
            };
            Some(CardInfo {
                cols: num("cols")?,
                rows: num("rows")?.min(tiles_term::block::MAX_ROWS),
                png: Rc::new(png),
                size_px,
                frames: small("frames").min(MAX_FRAMES),
                frame_ms: small("frame_ms").min(1000),
            })
        })();
        // A card the block names but this machine never made stays text;
        // look again later in case it is still being written.
        if info.is_some() {
            self.loaded.insert(id.to_string(), info.clone());
        }
        info
    }

    fn band(&mut self, id: &str, first: u16, count: u16) -> Option<Rgba> {
        let card = self.card(id)?;
        let img = self.rgba(id)?;
        let per_row = img.height / u32::from(card.rows.max(1));
        Some(img.rows(u32::from(first) * per_row, u32::from(count) * per_row))
    }

    fn band_png(&mut self, id: &str, first: u16, count: u16) -> Option<Vec<u8>> {
        let band = self.band(id, first, count)?;
        tiles_engine::frame_png(&tiles_engine::Frame {
            width: i32::try_from(band.width).ok()?,
            height: i32::try_from(band.height).ok()?,
            rgba: band.data,
        })
    }

    fn frames(&mut self, id: &str) -> Vec<Vec<u8>> {
        let Some(card) = self.card(id) else {
            return Vec::new();
        };
        (0..card.frames)
            .map(|n| self.read(&format!("{id}-anim-{n:02}.png")))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    }

    fn frame_band_png(&mut self, id: &str, n: u16, first: u16, count: u16) -> Option<Vec<u8>> {
        let card = self.card(id)?;
        if n >= card.frames {
            return None;
        }
        let png = self.read(&format!("{id}-anim-{n:02}.png"))?;
        if first == 0 && count >= card.rows {
            return Some(png);
        }
        let frame = decode(&png)?;
        let per_row = frame.height / u32::from(card.rows.max(1));
        let band = frame.rows(u32::from(first) * per_row, u32::from(count) * per_row);
        tiles_engine::frame_png(&tiles_engine::Frame {
            width: i32::try_from(band.width).ok()?,
            height: i32::try_from(band.height).ok()?,
            rgba: band.data,
        })
    }

    fn flat_png(&mut self, w: u32, h: u32, rgb: (u8, u8, u8)) -> Option<Vec<u8>> {
        let (iw, ih) = (i32::try_from(w).ok()?, i32::try_from(h).ok()?);
        if !(1..=MAX_SIDE).contains(&iw)
            || !(1..=MAX_SIDE).contains(&ih)
            || i64::from(iw) * i64::from(ih) > MAX_PIXELS
        {
            return None;
        }
        let pixel = [rgb.0, rgb.1, rgb.2, 255];
        tiles_engine::frame_png(&tiles_engine::Frame {
            width: iw,
            height: ih,
            rgba: pixel.repeat(usize::try_from(w).ok()? * usize::try_from(h).ok()?),
        })
    }

    fn fresh(&mut self) -> Vec<String> {
        // Listed again only when the directory changed: a card's geometry
        // file is written last, after its picture and frames. Another
        // session's cards share the cache but never reach this window.
        let changed = std::fs::metadata(&self.cards)
            .and_then(|m| m.modified())
            .ok();
        if changed.is_none() || changed == self.listed {
            return Vec::new();
        }
        self.listed = changed;
        let Ok(entries) = std::fs::read_dir(&self.cards) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| tiles_term::block::is_card_id(s))
            else {
                continue;
            };
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Ok(written) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if written < self.since || self.seen.get(id) == Some(&written) {
                continue;
            }
            let Some(ours) = self.ours(id) else {
                continue;
            };
            let again = self.seen.insert(id.to_string(), written).is_some();
            if again {
                // Drawn again under its id: read it afresh.
                self.loaded.remove(id);
                self.pixels.remove(id);
            }
            match (ours, again) {
                (true, false) => out.push(id.to_string()),
                (true, true) => self.redrawn.push(id.to_string()),
                (false, _) => {}
            }
        }
        out.sort();
        self.redrawn.sort();
        self.redrawn.dedup();
        out
    }

    fn redrawn(&mut self) -> Vec<String> {
        std::mem::take(&mut self.redrawn)
    }
}

#[cfg(test)]
mod tests {
    use super::{CachePictures, MAX_SIDE, decode};
    use crate::wrap::paint::Pictures as _;

    /// A PNG of one opaque white pixel's header and data, with its size
    /// claimed as `w` by `h`.
    fn png(w: u32, h: u32) -> Vec<u8> {
        fn crc(bytes: &[u8]) -> u32 {
            let mut c = 0xffff_ffffu32;
            for b in bytes {
                c ^= u32::from(*b);
                for _ in 0..8 {
                    c = if c & 1 == 1 {
                        0xedb8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
            let body = [kind, data].concat();
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc(&body).to_be_bytes());
        }
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let ihdr = [&w.to_be_bytes()[..], &h.to_be_bytes(), &[8, 2, 0, 0, 0]].concat();
        chunk(&mut out, b"IHDR", &ihdr);
        // zlib stored block holding one scanline: filter 0 and an RGB pixel.
        let idat = [
            0x78, 0x01, 0x01, 0x04, 0x00, 0xfb, 0xff, 0, 255, 255, 255, 0x05, 0xfe, 0x02, 0xfe,
        ];
        chunk(&mut out, b"IDAT", &idat);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn new_cards_are_listed_once_with_their_frames() {
        let dir = std::env::temp_dir().join(format!("tiles-pictures-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        let mut pictures = CachePictures::new(&dir, 7);
        assert_eq!(pictures.fresh(), Vec::<String>::new());
        std::fs::write(dir.join("cards/a1b2c3.png"), png(64, 32)).unwrap();
        std::fs::write(dir.join("cards/a1b2c3-anim-00.png"), b"f0").unwrap();
        std::fs::write(dir.join("cards/a1b2c3-anim-01.png"), b"f1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            dir.join("cards/a1b2c3.json"),
            r#"{"cols": 4, "rows": 1, "frames": 2, "frame_ms": 39, "session": "0000000000000007"}"#,
        )
        .unwrap();
        // Another session's card in the same cache, and one made outside
        // any session, are not sent ahead.
        std::fs::write(dir.join("cards/d4e5f6.png"), png(64, 32)).unwrap();
        std::fs::write(
            dir.join("cards/d4e5f6.json"),
            r#"{"cols": 4, "rows": 1, "frames": 0, "frame_ms": 0, "session": "0000000000000008"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("cards/0a0b0c.png"), png(64, 32)).unwrap();
        std::fs::write(
            dir.join("cards/0a0b0c.json"),
            r#"{"cols": 4, "rows": 1, "frames": 0, "frame_ms": 0, "session": null}"#,
        )
        .unwrap();
        assert_eq!(pictures.fresh(), ["a1b2c3"]);
        assert_eq!(pictures.fresh(), Vec::<String>::new(), "listed once");
        let card = pictures.card("a1b2c3").unwrap();
        assert_eq!(
            (card.size_px, card.frames, card.frame_ms),
            ((64, 32), 2, 39)
        );
        assert_eq!(pictures.frames("a1b2c3"), [b"f0".to_vec(), b"f1".to_vec()]);
        std::fs::remove_file(dir.join("cards/a1b2c3-anim-01.png")).unwrap();
        assert_eq!(
            pictures.frames("a1b2c3"),
            Vec::<Vec<u8>>::new(),
            "all or none"
        );
        let flat = pictures.flat_png(8, 4, (10, 10, 10)).unwrap();
        let rgba = decode(&flat).unwrap();
        assert_eq!(&rgba.data[..4], &[10, 10, 10, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The security review's F9: any block on screen redrew any cached card,
    // so an old block in scrollback or a file the harness showed could
    // repaint a card from an earlier session or another `tiles run`.
    #[test]
    fn only_this_sessions_cards_are_drawn() {
        let dir = std::env::temp_dir().join(format!("tiles-pictures-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        let mut pictures = CachePictures::new(&dir, 7);
        let card = |id: &str, session: &str| {
            std::fs::write(dir.join(format!("cards/{id}.png")), png(64, 32)).unwrap();
            std::fs::write(
                dir.join(format!("cards/{id}.json")),
                format!(r#"{{"cols": 4, "rows": 1, "session": {session}}}"#),
            )
            .unwrap();
        };
        card("0000bb", "null");
        card("0000cc", r#""0000000000000007""#);
        card("0000dd", r#""0000000000000008""#);
        assert!(pictures.card("0000bb").is_none(), "made for no session");
        assert!(pictures.card("0000cc").is_some(), "made for this session");
        assert!(pictures.card("0000dd").is_none(), "another session's card");
        assert!(pictures.band("0000dd", 0, 1).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_png_decodes() {
        let rgba = decode(&png(1, 1)).expect("decodes");
        assert_eq!((rgba.width, rgba.height), (1, 1));
    }

    // The security review's F11: the header alone decided the allocation.
    #[test]
    fn a_png_claiming_a_huge_size_is_refused_before_allocating() {
        assert!(decode(&png(65_535, 65_535)).is_none());
        let side = u32::try_from(MAX_SIDE).unwrap();
        assert!(decode(&png(side + 1, 1)).is_none());
        assert!(decode(&png(side, side)).is_none(), "over the pixel budget");
    }
}
