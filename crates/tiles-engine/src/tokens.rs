//! Design tokens: the card's numbers and colours (ADR-002), sealed into the
//! build from a design pack or, without one, from the plain theme in
//! `assets/plain/tokens.json` (ADR-036, ADR-037). Sizes are CSS pixels at
//! scale 1.

use serde::Deserialize;

use crate::color::Rgba;

/// Every number and colour the card and marks use.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    /// Card chrome.
    pub card: CardTokens,
    /// Type sizes and weights.
    #[serde(rename = "type")]
    pub text: TypeTokens,
    /// Mark sizes.
    pub marks: MarkTokens,
    /// Colours per theme.
    pub themes: Themes,
    /// The entrance.
    pub motion: MotionTokens,
}

/// Card chrome (playbook C1 to C5).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // Each field is the playbook value of the same name.
pub struct CardTokens {
    pub radius: f32,
    pub padding: f32,
    pub border: f32,
    pub margin_x: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
}

/// Type (playbook T1 to T6).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // Each field is the playbook value of the same name.
pub struct TypeTokens {
    pub title_size: f32,
    pub title_line: f32,
    pub title_weight: u16,
    pub title_tracking_em: f32,
    pub subtitle_size: f32,
    pub subtitle_line: f32,
    pub subtitle_gap: f32,
    pub chart_gap: f32,
    pub label_size: f32,
    pub label_weight: u16,
    pub value_size: f32,
    pub value_weight: u16,
    pub row_value_weight: u16,
    pub legend_size: f32,
    pub big_size: f32,
    pub big_weight: u16,
    pub delta_size: f32,
    pub delta_weight: u16,
}

/// Marks (playbook M1 to M5).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // Each field is the playbook value of the same name.
pub struct MarkTokens {
    pub plot_height: f32,
    pub band_padding_inner: f32,
    pub band_padding_outer: f32,
    pub row_step: f32,
    pub bar_band: f32,
    pub bar_radius: f32,
    pub value_offset: f32,
    pub row_value_offset: f32,
    pub label_padding: f32,
    pub line_width: f32,
    pub line_opacity: f32,
    pub line_width_named: f32,
    pub line_width_rest: f32,
    pub point_radius: f32,
    pub point_radius_highlight: f32,
    pub point_opacity: f32,
    pub area_opacity: f32,
    pub stem_width: f32,
    pub dot_radius: f32,
    pub head_radius: f32,
    pub grid_width: f32,
    pub legend_swatch: f32,
    pub legend_gap: f32,
    pub legend_height: f32,
    pub spark_height: f32,
    pub tick_spacing: f32,
}

/// Most themes one tokens file carries.
pub const MAX_THEMES: usize = 16;

/// Every theme the tokens carry (ADR-036): a design pack's, or the source
/// tree's.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Themes {
    /// The name of the theme a card is drawn in when the user picks none.
    pub default: String,
    /// The themes, in the order `tiles theme` lists them.
    pub list: Vec<Palette>,
}

impl Themes {
    /// Every theme's palette.
    pub fn all(&self) -> impl Iterator<Item = &Palette> {
        self.list.iter()
    }

    /// The theme of this name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Palette> {
        self.list.iter().find(|p| p.name == name)
    }

    /// The default theme's palette, which [`Tokens::from_json`] checks is
    /// in the list.
    #[must_use]
    pub fn default_palette(&self) -> &Palette {
        self.get(&self.default).unwrap_or(&self.list[0])
    }
}

/// One theme: its name, a few words on its look, and its colours (playbook
/// K1 to K6, C3, C4, C6).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // Each colour is the playbook colour of the same name.
pub struct Palette {
    /// The name `tiles theme` and `--theme` take: lowercase letters,
    /// digits and dashes.
    pub name: String,
    /// What the theme looks like, for the theme list.
    pub about: String,
    pub card: Rgba,
    pub line: Rgba,
    pub ink: Rgba,
    pub muted: Rgba,
    pub grey: Rgba,
    pub accent: Rgba,
    pub bad: Rgba,
    pub bad_soft: Rgba,
    pub grid: Rgba,
    pub track: Rgba,
    pub categorical: Vec<Rgba>,
    pub neutrals: Vec<Rgba>,
    /// The ordered greys of the lines beside a named line (K1, ADR-047):
    /// the first unnamed line takes the first, and every line past the
    /// list takes the last.
    pub context: Vec<Rgba>,
    pub shadows: Vec<Shadow>,
    /// Most series told apart by colour: the shortest categorical list of
    /// any theme, so a chart one theme draws every theme draws (ADR-021).
    /// Set by [`Tokens::from_json`]; colours past it in a longer list stay
    /// unused.
    #[serde(skip)]
    pub series: usize,
}

/// One shadow layer under the card.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    /// Vertical offset.
    pub dy: f32,
    /// CSS blur radius.
    pub blur: f32,
    /// Opacity of black.
    pub alpha: f32,
}

/// The entrance (playbook N1) and its reduced form (N2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // Each field is the playbook value of the same name.
pub struct MotionTokens {
    /// Milliseconds from one entrance frame to the next.
    pub frame_ms: f32,
    pub duration_ms: f32,
    pub fade_ms: f32,
    /// The fade's `cubic-bezier`, also the reduced entrance's.
    pub fade_ease: [f32; 4],
    pub float_ms: f32,
    pub float_px: f32,
    pub start_scale: f32,
    pub marks_start_ms: f32,
    pub marks_end_ms: f32,
    pub labels_start_ms: f32,
    pub labels_end_ms: f32,
    /// The value labels' `cubic-bezier`.
    pub labels_ease: [f32; 4],
    /// How long the reduced entrance's fade takes.
    pub reduce_ms: f32,
    /// The reduced fade's `cubic-bezier`: gentler than the full
    /// entrance's, so its few frames show a fade rather than a pop.
    pub reduce_ease: [f32; 4],
}

impl MotionTokens {
    /// A one-line reason when a value is out of range.
    fn check(&self) -> Result<(), String> {
        let positive = [
            self.duration_ms,
            self.fade_ms,
            self.float_ms,
            self.reduce_ms,
            self.marks_end_ms - self.marks_start_ms,
            self.labels_end_ms - self.labels_start_ms,
        ];
        let curves = [self.fade_ease, self.labels_ease, self.reduce_ease];
        if !(10.0..=1000.0).contains(&self.frame_ms) {
            return Err("tokens.json: motion.frame_ms must be 10 to 1000".into());
        }
        if positive.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err("tokens.json: motion times and spans must be positive".into());
        }
        if curves.iter().flatten().any(|v| !v.is_finite())
            || curves
                .iter()
                .any(|c| !(0.0..=1.0).contains(&c[0]) || !(0.0..=1.0).contains(&c[2]))
        {
            return Err("tokens.json: a motion curve's x values must be 0 to 1".into());
        }
        Ok(())
    }
}

impl Themes {
    /// A one-line reason when the list is empty or too long, a name is not a theme name
    /// or comes twice, the default is not in the list, or a theme has no
    /// categorical, neutral or context colours.
    fn check(&self) -> Result<(), String> {
        if !(1..=MAX_THEMES).contains(&self.list.len()) {
            return Err(format!(
                "tokens.json: themes.list needs 1 to {MAX_THEMES} themes"
            ));
        }
        for (i, p) in self.list.iter().enumerate() {
            let name = &p.name;
            if !valid_name(name) {
                return Err(format!(
                    "tokens.json: theme name \"{name}\" must be 1 to 24 lowercase letters, digits and dashes"
                ));
            }
            if self.list[..i].iter().any(|q| q.name == *name) {
                return Err(format!("tokens.json: theme \"{name}\" is listed twice"));
            }
            if p.categorical.is_empty() || p.neutrals.is_empty() || p.context.is_empty() {
                return Err(format!(
                    "tokens.json: theme {name} needs categorical, neutrals and context colours"
                ));
            }
        }
        if self.get(&self.default).is_none() {
            return Err(format!(
                "tokens.json: themes.default \"{}\" is not in themes.list",
                self.default
            ));
        }
        Ok(())
    }
}

/// Whether `name` can name a theme: 1 to 24 lowercase letters, digits and
/// dashes, so it is safe to print and to use in a file name.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    (1..=24).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Tokens {
    /// Parses and checks a tokens file.
    ///
    /// # Errors
    ///
    /// A one-line reason when the JSON does not match or a value is out of range.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut t: Tokens = serde_json::from_str(json).map_err(|e| format!("tokens.json: {e}"))?;
        t.themes.check()?;
        t.motion.check()?;
        if t.marks.plot_height <= 0.0 || t.marks.row_step <= 0.0 || t.marks.tick_spacing <= 0.0 {
            return Err("tokens.json: marks sizes must be positive".into());
        }
        let series = t
            .themes
            .all()
            .map(|p| p.categorical.len())
            .min()
            .unwrap_or(1);
        for p in &mut t.themes.list {
            p.series = series;
        }
        Ok(t)
    }

    /// The palette for a theme; the default theme's when these tokens
    /// carry no theme of that name, as a `$TILES_DEV_ASSETS` directory may
    /// not.
    #[must_use]
    pub fn palette(&self, theme: crate::Theme) -> &Palette {
        self.themes
            .get(theme.name())
            .unwrap_or_else(|| self.themes.default_palette())
    }
}
