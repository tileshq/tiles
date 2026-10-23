//! sRGB colours with alpha, parsed from `#rrggbb` in the tokens file.

use serde::{Deserialize, Deserializer};

/// An sRGB colour with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
}

impl Rgba {
    /// Parses `#rrggbb`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: 255,
        })
    }

    /// Opaque black at the given opacity.
    #[must_use]
    pub fn black(alpha: f32) -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: unit_to_byte(alpha),
        }
    }

    /// The same colour at `opacity` times its alpha.
    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Self {
        Self {
            a: unit_to_byte(f32::from(self.a) / 255.0 * opacity),
            ..self
        }
    }

    /// Whether black stands out more on it than white does: a light colour.
    #[must_use]
    pub fn is_light(self) -> bool {
        let white = Self {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        self.contrast(Self::black(1.0)) > self.contrast(white)
    }

    /// The WCAG contrast ratio between two opaque colours, 1 to 21.
    #[must_use]
    pub fn contrast(self, other: Self) -> f32 {
        let lum = |c: Self| {
            let ch = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.040_45 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b)
        };
        let (a, b) = (lum(self), lum(other));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `#rrggbb`, ignoring alpha.
    #[must_use]
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Maps 0..1 to 0..255, clamped.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to 0..=255 first.
pub fn unit_to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgba::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("{s:?} is not #rrggbb")))
    }
}

#[cfg(test)]
mod tests {
    use super::Rgba;

    #[test]
    fn parses_hex() {
        let c = Rgba::parse("#3a6fd8").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (0x3a, 0x6f, 0xd8, 255));
        assert_eq!(c.hex(), "#3a6fd8");
        assert!(Rgba::parse("3a6fd8").is_none());
        assert!(Rgba::parse("#3a6fd").is_none());
        assert!(Rgba::parse("#zz6fd8").is_none());
        assert!(Rgba::parse("#ééé").is_none());
    }

    #[test]
    fn contrast_runs_from_one_to_twenty_one() {
        let white = Rgba::parse("#ffffff").unwrap();
        let black = Rgba::parse("#000000").unwrap();
        assert!((white.contrast(black) - 21.0).abs() < 0.01);
        assert!((black.contrast(black) - 1.0).abs() < 0.001);
        let accent = Rgba::parse("#3a6fd8").unwrap();
        let ink = Rgba::parse("#1c2430").unwrap();
        assert!(accent.contrast(white) > accent.contrast(ink));
    }

    #[test]
    fn opacity_scales_alpha() {
        assert_eq!(Rgba::parse("#000000").unwrap().with_opacity(0.25).a, 64);
    }
}
