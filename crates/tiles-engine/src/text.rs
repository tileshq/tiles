//! Text: bundled Inter, shaped by Skia's paragraph module (`HarfBuzz`), drawn
//! as glyph outlines.
//!
//! The glyphs become paths filled by Skia's own rasterizer, so the platform's
//! font scaler (Core Text on macOS, `FreeType` on Linux) never touches a pixel
//! and the same intent draws the same text on every machine (ADR-002). Inter
//! sets every character it has a glyph for. On macOS a character it lacks
//! comes from one of three system fonts, [`SYSTEM_FAMILIES`], and from no
//! other: Skia's own fallback stays off (playbook T1, ADR-050). Their glyphs
//! are outlines too, except Apple Color Emoji's, which are pictures drawn as
//! they are.

use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextStyle as SkTextStyle,
    TypefaceFontProvider,
};
use skia_safe::{
    Data, Font, FontHinting, FontMgr, FontStyle, GlyphId, Path, PathBuilder, Point, Typeface,
    font_style,
};

const INTER_400: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_500: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
const INTER_600: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const FAMILY: &str = "Inter";
/// The macOS fonts that set a character Inter has no glyph for, tried in
/// this order: Chinese, Japanese and Korean, then Arabic, then emoji
/// (ADR-050).
pub const SYSTEM_FAMILIES: [&str; 3] = ["PingFang SC", "Geeza Pro", "Apple Color Emoji"];
/// The one system font whose glyphs are colour pictures, not outlines.
const EMOJI_FAMILY: &str = "Apple Color Emoji";
/// Words that separate the parts of a title or subtitle, kept on the line of
/// the word before them.
const SEPARATORS: [&str; 4] = ["·", "•", "|", "—"];

/// How a run of text is set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Font size in CSS pixels.
    pub size: f32,
    /// Weight: 400, 500 or 600; anything heavier draws 600 (playbook T1).
    pub weight: u16,
    /// Tracking in em.
    pub tracking_em: f32,
    /// Tabular figures (playbook T6).
    pub tabular: bool,
}

impl Style {
    /// A plain style at a size and weight.
    #[must_use]
    pub fn new(size: f32, weight: u16) -> Self {
        Self {
            size,
            weight,
            tracking_em: 0.0,
            tabular: false,
        }
    }

    /// The same style at another weight.
    #[must_use]
    pub fn with_weight(self, weight: u16) -> Self {
        Self { weight, ..self }
    }

    /// The same style with tabular figures.
    #[must_use]
    pub fn tabular(self) -> Self {
        Self {
            tabular: true,
            ..self
        }
    }

    /// The bundled weight this style draws with.
    #[must_use]
    pub fn drawn_weight(self) -> u16 {
        match self.weight {
            0..=449 => 400,
            450..=549 => 500,
            _ => 600,
        }
    }
}

/// One system font the engine draws with, named for the golden tests: its
/// family, the weight it was asked for, and a digest of its `head` table,
/// which changes with the font's version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFont {
    /// The family, one of [`SYSTEM_FAMILIES`].
    pub family: String,
    /// The weight asked for: 400, 500 or 600.
    pub weight: u16,
    /// Hex SHA-256 of the font's `head` table.
    pub head: String,
}

/// Colour glyphs of one font, placed relative to the text's start on the
/// baseline.
#[derive(Debug, Clone)]
pub struct ColourRun {
    /// The font at the text's size.
    pub font: Font,
    /// The glyphs.
    pub glyphs: Vec<GlyphId>,
    /// Where each glyph's origin goes.
    pub points: Vec<Point>,
}

/// One line of text ready to draw: the outline of every glyph that has one,
/// and the colour glyphs that do not.
#[derive(Debug, Clone)]
pub struct Shaped {
    /// The outlines, filled in the text's colour.
    pub outline: Path,
    /// Colour emoji, drawn in their own colours.
    pub colour: Vec<ColourRun>,
}

impl Shaped {
    /// The same text moved by `(dx, dy)`.
    #[must_use]
    pub fn with_offset(&self, (dx, dy): (f32, f32)) -> Self {
        Self {
            outline: self.outline.with_offset((dx, dy)),
            colour: self
                .colour
                .iter()
                .map(|r| ColourRun {
                    font: r.font.clone(),
                    glyphs: r.glyphs.clone(),
                    points: r.points.iter().map(|p| *p + Point::new(dx, dy)).collect(),
                })
                .collect(),
        }
    }
}

/// The bundled fonts, any system fonts behind them, and a shaper over them.
pub struct Fonts {
    collection: FontCollection,
    // Inter, then each system family that loaded.
    families: Vec<&'static str>,
    system: Vec<SystemFont>,
    inter: Typeface,
    // Ascent and descent per em, from the font's own tables.
    ascent_em: f32,
    descent_em: f32,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").finish_non_exhaustive()
    }
}

impl Fonts {
    /// Loads the bundled Inter weights and, on macOS, the system fonts in
    /// [`SYSTEM_FAMILIES`] that this machine has.
    ///
    /// # Errors
    ///
    /// When Skia cannot read a bundled font.
    pub fn new() -> Result<Self, String> {
        Self::load(cfg!(target_os = "macos"))
    }

    /// Loads the bundled Inter weights alone, as on a machine without the
    /// system fonts: a character Inter lacks draws as an empty box.
    ///
    /// # Errors
    ///
    /// When Skia cannot read a bundled font.
    pub fn inter_only() -> Result<Self, String> {
        Self::load(false)
    }

    fn load(system: bool) -> Result<Self, String> {
        let mgr = FontMgr::new();
        let mut provider = TypefaceFontProvider::new();
        let mut ascent_em = 0.0;
        let mut descent_em = 0.0;
        let mut inter = None;
        for (bytes, weight) in [(INTER_400, 400), (INTER_500, 500), (INTER_600, 600)] {
            let typeface = mgr
                .new_from_data(Data::new_copy(bytes), None)
                .ok_or_else(|| format!("the bundled Inter {weight} font could not be read"))?;
            if weight == 400 {
                let font = Font::from_typeface(&typeface, 1000.0);
                let (_, m) = font.metrics();
                ascent_em = -m.ascent / 1000.0;
                descent_em = m.descent / 1000.0;
                inter = Some(typeface.clone());
            }
            provider.register_typeface(typeface, Some(FAMILY));
        }
        let mut families = vec![FAMILY];
        let mut loaded = Vec::new();
        if system {
            for family in SYSTEM_FAMILIES {
                let mut seen = Vec::new();
                for weight in [400_u16, 500, 600] {
                    let style = FontStyle::new(
                        font_style::Weight::from(i32::from(weight)),
                        font_style::Width::NORMAL,
                        font_style::Slant::Upright,
                    );
                    // A font manager can answer with a stand-in family; only
                    // the named one is used.
                    let Some(typeface) = mgr
                        .match_family_style(family, style)
                        .filter(|t| t.family_name() == family)
                    else {
                        continue;
                    };
                    loaded.push(SystemFont {
                        family: family.to_string(),
                        weight,
                        head: head_digest(&typeface),
                    });
                    if !seen.contains(&typeface.unique_id()) {
                        seen.push(typeface.unique_id());
                        provider.register_typeface(typeface, Some(family));
                    }
                }
                if !seen.is_empty() {
                    families.push(family);
                }
            }
        }
        let mut collection = FontCollection::new();
        collection.set_asset_font_manager(Some(FontMgr::from(provider)));
        collection.disable_font_fallback();
        Ok(Self {
            collection,
            families,
            system: loaded,
            inter: inter.ok_or("the bundled Inter 400 font could not be read")?,
            ascent_em,
            descent_em,
        })
    }

    /// The system fonts loaded behind Inter, empty off macOS or with
    /// [`Fonts::inter_only`].
    #[must_use]
    pub fn system_fonts(&self) -> &[SystemFont] {
        &self.system
    }

    /// Whether `text` holds a character Inter has no glyph for.
    #[must_use]
    pub fn beyond_inter(&self, text: &str) -> bool {
        text.chars().any(|c| {
            !c.is_control() && !c.is_whitespace() && self.inter.unichar_to_glyph(c as i32) == 0
        })
    }

    fn paragraph(&self, text: &str, style: Style, max_width: f32) -> Paragraph {
        let mut ts = SkTextStyle::new();
        ts.set_font_families(&self.families);
        ts.set_font_size(style.size);
        let weight = font_style::Weight::from(i32::from(style.drawn_weight()));
        ts.set_font_style(FontStyle::new(
            weight,
            font_style::Width::NORMAL,
            font_style::Slant::Upright,
        ));
        ts.set_letter_spacing(style.tracking_em * style.size);
        if style.tabular {
            ts.add_font_feature("tnum", 1);
        }
        let mut ps = ParagraphStyle::new();
        ps.set_text_style(&ts);
        ps.set_max_lines(1);
        ps.set_apply_rounding_hack(false);
        // Unhinted advances: FreeType would otherwise snap them on Linux while
        // Core Text never does, and the glyphs would land differently.
        ps.turn_hinting_off();
        let mut builder = ParagraphBuilder::new(&ps, self.collection.clone());
        builder.add_text(text);
        let mut p = builder.build();
        p.layout(max_width);
        p
    }

    /// The advance width of one line of text.
    #[must_use]
    pub fn width(&self, text: &str, style: Style) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.paragraph(text, style, 100_000.0).max_intrinsic_width()
    }

    /// Distance from a line box's top to its baseline, as Chrome sets it:
    /// ascent and descent each rounded to whole pixels, and the leading split
    /// above and below with the odd pixel going below.
    #[must_use]
    pub fn baseline_in_line(&self, size: f32, line_height: f32) -> f32 {
        let ascent = (self.ascent_em * size).round();
        let content = ascent + (self.descent_em * size).round();
        ((line_height - content) / 2.0).floor() + ascent
    }

    /// Ascent and descent per em.
    #[must_use]
    pub fn vertical_metrics(&self) -> (f32, f32) {
        (self.ascent_em, self.descent_em)
    }

    /// One line of text as glyph outlines and colour glyphs, with the
    /// baseline at y = 0 and the left edge at x = 0.
    #[must_use]
    pub fn outline(&self, text: &str, style: Style) -> Shaped {
        let mut out = PathBuilder::new();
        let mut colour = Vec::new();
        if text.is_empty() {
            return Shaped {
                outline: out.detach(),
                colour,
            };
        }
        let mut p = self.paragraph(text, style, 100_000.0);
        let baseline = p.alphabetic_baseline();
        p.visit(|_, info| {
            let Some(info) = info else { return };
            let mut font = info.font().clone();
            // Unhinted outlines at the exact size: no scaler-specific adjustment.
            font.set_hinting(FontHinting::None);
            font.set_subpixel(true);
            font.set_linear_metrics(true);
            let origin = info.origin();
            let at = |pos: &Point| Point::new(origin.x + pos.x, origin.y + pos.y - baseline);
            if font.typeface().family_name() == EMOJI_FAMILY {
                colour.push(ColourRun {
                    glyphs: info.glyphs().to_vec(),
                    points: info.positions().iter().map(at).collect(),
                    font,
                });
                return;
            }
            for (glyph, pos) in info.glyphs().iter().zip(info.positions()) {
                if let Some(path) = font.get_path(*glyph) {
                    out.add_path_with_offset(&path, at(pos), None);
                }
            }
        });
        Shaped {
            outline: out.detach(),
            colour,
        }
    }

    /// Wraps text to `max_width` in as few lines as a greedy fill needs, then
    /// narrows the measure until the lines are as even as that count allows
    /// (CSS `text-wrap: balance`).
    #[must_use]
    pub fn wrap_balanced(&self, text: &str, style: Style, max_width: f32) -> Vec<String> {
        // A separator stays with the word before it, so no line starts with
        // one: "data ·" / "Nov 2025", never "data" / "· Nov 2025".
        let mut words: Vec<String> = Vec::new();
        for word in text.split_whitespace() {
            match words.last_mut() {
                Some(last) if SEPARATORS.contains(&word) => {
                    last.push(' ');
                    last.push_str(word);
                }
                _ => words.push(word.to_string()),
            }
        }
        let widths: Vec<f32> = words.iter().map(|w| self.width(w, style)).collect();
        let space = self.width("a a", style) - self.width("aa", style);
        let greedy = |limit: f32| -> Vec<String> {
            let mut lines: Vec<String> = Vec::new();
            let mut current = String::new();
            let mut w = 0.0;
            for (word, ww) in words.iter().zip(&widths) {
                if !current.is_empty() && w + space + ww > limit {
                    lines.push(std::mem::take(&mut current));
                    w = 0.0;
                }
                if !current.is_empty() {
                    current.push(' ');
                    w += space;
                }
                current.push_str(word);
                w += *ww;
            }
            if !current.is_empty() {
                lines.push(current);
            }
            lines
        };
        let lines = greedy(max_width);
        if lines.len() <= 1 {
            return lines;
        }
        let (mut lo, mut hi) = (0.0_f32, max_width);
        for _ in 0..14 {
            let mid = f32::midpoint(lo, hi);
            if greedy(mid).len() <= lines.len() {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        greedy(hi)
    }
}

/// Hex SHA-256 of a typeface's `head` table, or of nothing when it has none.
fn head_digest(typeface: &Typeface) -> String {
    let tag = u32::from_be_bytes(*b"head");
    let mut table = vec![0; typeface.get_table_size(tag).unwrap_or(0)];
    let n = typeface.get_table_data(tag, &mut table);
    table.truncate(n);
    crate::sha256::hex(&crate::sha256::digest(&table))
}

#[cfg(test)]
mod tests {
    use super::{Fonts, Style};

    #[test]
    fn a_separator_never_starts_a_line() {
        let fonts = Fonts::new().unwrap();
        let s = Style::new(11.5, 400);
        let text = "Made-up example data · Nov 2025–Oct 2026";
        let mut wrapped = 0;
        for width in [120.0, 150.0, 180.0, 200.0, 240.0] {
            let lines = fonts.wrap_balanced(text, s, width);
            wrapped += usize::from(lines.len() > 1);
            for line in &lines {
                assert!(!line.starts_with('·'), "{width}: {lines:?}");
            }
        }
        assert!(wrapped >= 3, "the widths wrap the subtitle");
    }

    #[test]
    fn heavier_weights_draw_semibold() {
        assert_eq!(Style::new(15.0, 650).drawn_weight(), 600);
        assert_eq!(Style::new(15.0, 400).drawn_weight(), 400);
        assert_eq!(Style::new(15.0, 500).drawn_weight(), 500);
    }

    #[test]
    fn tabular_figures_have_equal_widths() {
        let fonts = Fonts::new().unwrap();
        let s = Style::new(11.0, 600).tabular();
        let one = fonts.width("1111", s);
        let eight = fonts.width("8888", s);
        assert!((one - eight).abs() < 0.01, "{one} vs {eight}");
        let p = Style::new(11.0, 600);
        assert!(fonts.width("1111", p) < fonts.width("8888", p));
    }

    #[test]
    fn balanced_wrap_evens_the_lines() {
        let fonts = Fonts::new().unwrap();
        let s = Style::new(15.0, 600);
        let text = "Enterprise signups grew fastest in the north region while the south fell";
        let lines = fonts.wrap_balanced(text, s, 400.0);
        assert_eq!(lines.len(), 2);
        let w: Vec<f32> = lines.iter().map(|l| fonts.width(l, s)).collect();
        assert!((w[0] - w[1]).abs() < 80.0, "{lines:?} {w:?}");
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn outlines_are_non_empty_and_start_near_zero() {
        let fonts = Fonts::new().unwrap();
        let path = fonts.outline("W3", Style::new(11.0, 400)).outline;
        let b = path.bounds();
        assert!(
            b.width() > 5.0 && b.left >= -1.0 && b.bottom <= 1.0 && b.top < -5.0,
            "{b:?}"
        );
    }
}
