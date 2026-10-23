//! Native chart renderer for Tiles (ADR-002).
//!
//! The pipeline is intent to scene to pixels. [`Engine::scene`] validates an
//! intent against its form and lays out every mark and text in CSS pixels;
//! [`Engine::png`] draws the settled card, [`Engine::frames`] the entrance,
//! and [`hit::hit_map`] the hover targets, all from the same [`Scene`].
//!
//! The design tokens and form definitions are data files embedded in the
//! binary, sealed at build time and opened in memory at load (ADR-022); a
//! build can take them sealed in a design pack instead ([`pack`], ADR-036). A
//! debug build reads them from `$TILES_DEV_ASSETS` instead when that
//! variable names a directory: `tokens.json` from it, and the forms from its
//! `forms/` when it has one or the embedded forms otherwise, so
//! `crates/tiles-engine/assets/plain` loads what a build from source seals
//! (ADR-037) and a form or a taste change is tried without a recompile; the
//! release binary never reads them from disk.

mod chacha;
pub mod color;
pub mod format;
pub mod forms;
pub mod hit;
mod layout;
pub mod motion;
pub mod pack;
mod raster;
pub mod scale;
pub mod scene;
mod seal;
mod sha256;
pub mod text;
pub mod tokens;

use tiles_intent::{Intent, IntentError};

pub use forms::Form;
pub use layout::MAX_CATEGORIES;
pub use motion::{Entrance, Moment};
pub use scene::{Board, Placed, Scene};
pub use tokens::Tokens;

/// The embedded design tokens, opened from their sealed copy.
///
/// # Errors
///
/// When the sealed copy does not open.
pub fn embedded_tokens() -> Result<String, String> {
    seal::open("tokens.json", seal::TOKENS)
}

/// The tokens file in `dir`, or the embedded one when `dir` is `None`.
fn tokens_json(dir: Option<&std::path::Path>) -> Result<String, String> {
    match dir {
        Some(d) => std::fs::read_to_string(d.join("tokens.json"))
            .map_err(|e| format!("{}: {e}", d.display())),
        None => embedded_tokens(),
    }
}

/// Where this build's sealed assets came from (ADR-036): `assets`, the
/// source tree, or `pack <n>`, design pack `n` through `TILES_DESIGN_PACK`.
pub const SEAL_SOURCE: &str = env!("TILES_SEAL_SOURCE");

/// The environment variable a debug build reads assets from.
pub const DEV_ASSETS_VAR: &str = "TILES_DEV_ASSETS";
/// Widest tile drawn, in CSS pixels.
pub const MAX_WIDTH: f32 = 2400.0;
/// Narrowest tile drawn, in CSS pixels.
pub const MIN_WIDTH: f32 = 280.0;
/// Widest and tallest board drawn, in CSS pixels: four of the widest tiles.
pub const MAX_BOARD: f32 = 4.0 * MAX_WIDTH;
/// Largest device scale.
pub const MAX_SCALE: f32 = 4.0;
/// Milliseconds between two cards' entrances on a board: one wrapper tick.
const STAGGER_MS: f32 = 50.0;
/// Most milliseconds between the first card's entrance and the last's.
const MAX_STAGGER_MS: f32 = 200.0;

/// A card theme the build carries: one of the design tokens' themes, by
/// name (ADR-021, ADR-036). Cards are drawn in the tokens' default theme
/// unless the user picks another. The themes are read once, from the tokens
/// [`Engine::new`] loads.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Theme(u8);

/// The build's themes, in list order.
struct Catalog {
    all: Vec<Theme>,
    entries: Vec<Entry>,
    default: Theme,
}

/// What [`Theme`] answers about one theme.
struct Entry {
    name: String,
    about: String,
    light: bool,
}

fn catalog() -> &'static Catalog {
    static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let tokens = tokens_json(dev_dir().as_deref())
            .ok()
            .and_then(|json| Tokens::from_json(&json).ok());
        // Tokens that do not load leave one stand-in theme; `Engine::new`
        // then reports why they did not.
        let Some(tokens) = tokens else {
            return Catalog {
                all: vec![Theme(0)],
                entries: vec![Entry {
                    name: "default".into(),
                    about: String::new(),
                    light: false,
                }],
                default: Theme(0),
            };
        };
        let entries: Vec<Entry> = tokens
            .themes
            .all()
            .map(|p| Entry {
                name: p.name.clone(),
                about: p.about.clone(),
                light: p.card.is_light(),
            })
            .collect();
        // `Tokens::from_json` allows at most `MAX_THEMES`, so every index fits.
        let theme = |i: usize| Theme(u8::try_from(i).unwrap_or(0));
        let default = entries
            .iter()
            .position(|e| e.name == tokens.themes.default)
            .map_or(Theme(0), theme);
        Catalog {
            all: (0..entries.len()).map(theme).collect(),
            entries,
            default,
        }
    })
}

impl Theme {
    /// Every theme, in the order `tiles theme` lists them.
    #[must_use]
    pub fn all() -> &'static [Theme] {
        &catalog().all
    }

    /// The theme's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        &catalog().entries[usize::from(self.0)].name
    }

    /// What the theme looks like, in a few words, for the theme list.
    #[must_use]
    pub fn about(self) -> &'static str {
        &catalog().entries[usize::from(self.0)].about
    }

    /// Whether the card is light, so what shows it around the card is too.
    #[must_use]
    pub fn is_light(self) -> bool {
        catalog().entries[usize::from(self.0)].light
    }

    /// The theme a name names, one of [`Theme::all`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Theme::all().iter().copied().find(|t| t.name() == name)
    }
}

impl Default for Theme {
    /// The design tokens' default theme.
    fn default() -> Self {
        catalog().default
    }
}

impl std::fmt::Debug for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Theme").field(&self.name()).finish()
    }
}

/// How a card is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Tile width in CSS pixels, margins included.
    pub width: f32,
    /// The theme.
    pub theme: Theme,
    /// A terminal cell's height in CSS pixels: the tile's height is rounded
    /// up to whole cells (playbook C5). `None` draws the natural height.
    pub cell_height: Option<f32>,
}

/// How a card lines up with the other cards of its row, so their titles,
/// headline numbers, change lines and sparklines sit on the same lines.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Align {
    /// A scorecard among others ([`Form::is_scorecard`]): no default
    /// subtitle, a tighter gap under the header, a short sparkline and no
    /// period labels under it, about half the height of a lone card.
    pub compact: bool,
    /// One of several cards on a board: no default subtitle, since the
    /// board's title and note are written once, above every card.
    pub among: bool,
    /// Lines the title takes at least.
    pub title_lines: usize,
    /// Lines the subtitle takes at least.
    pub subtitle_lines: usize,
    /// The tile's height at least, in CSS pixels, before it is rounded up
    /// to whole cells.
    pub min_height: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 640.0,
            theme: Theme::default(),
            cell_height: None,
        }
    }
}

/// Why a card could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The intent breaks a rule; the message names the key and the fix.
    Intent(IntentError),
    /// The engine's own assets or Skia failed.
    Engine(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Intent(e) => e.fmt(f),
            RenderError::Engine(e) => write!(f, "engine: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<IntentError> for RenderError {
    fn from(e: IntentError) -> Self {
        RenderError::Intent(e)
    }
}

/// One drawn frame of unpremultiplied RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Width in pixels.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
    /// Four bytes per pixel, row by row.
    pub rgba: Vec<u8>,
}

/// The loaded tokens, forms and fonts. Build one and reuse it.
#[derive(Debug)]
pub struct Engine {
    tokens: Tokens,
    forms: Vec<Form>,
    fonts: text::Fonts,
}

impl Engine {
    /// Loads the embedded assets, or the dev directory in a debug build.
    ///
    /// # Errors
    ///
    /// When an asset fails to parse or check, or a bundled font is unreadable.
    pub fn new() -> Result<Self, RenderError> {
        Self::with_assets(dev_dir().as_deref())
    }

    /// Loads assets from `dir` (`tokens.json`, and `forms/` when it has
    /// one), or the embedded copies when `dir` is `None`.
    ///
    /// # Errors
    ///
    /// When an asset fails to parse or check, or a bundled font is unreadable.
    pub fn with_assets(dir: Option<&std::path::Path>) -> Result<Self, RenderError> {
        let tokens_json = tokens_json(dir).map_err(RenderError::Engine)?;
        Ok(Self {
            tokens: Tokens::from_json(&tokens_json).map_err(RenderError::Engine)?,
            forms: forms::load(dir).map_err(RenderError::Engine)?,
            fonts: text::Fonts::new().map_err(RenderError::Engine)?,
        })
    }

    /// The same engine with the bundled Inter alone, as on a machine
    /// without the macOS system fonts (ADR-050): a character Inter lacks
    /// draws as an empty box.
    ///
    /// # Errors
    ///
    /// When a bundled font is unreadable.
    pub fn inter_only(self) -> Result<Self, RenderError> {
        Ok(Self {
            fonts: text::Fonts::inter_only().map_err(RenderError::Engine)?,
            ..self
        })
    }

    /// The system fonts this engine sets characters Inter lacks with, empty
    /// off macOS (ADR-050).
    #[must_use]
    pub fn system_fonts(&self) -> &[text::SystemFont] {
        self.fonts.system_fonts()
    }

    /// Whether `text` holds a character Inter has no glyph for.
    #[must_use]
    pub fn beyond_inter(&self, text: &str) -> bool {
        self.fonts.beyond_inter(text)
    }

    /// The forms, in the order they are listed.
    #[must_use]
    pub fn forms(&self) -> &[Form] {
        &self.forms
    }

    /// The advance width of one line of text in CSS pixels, as the engine
    /// sets it.
    #[must_use]
    pub fn text_width(&self, text: &str, style: text::Style) -> f32 {
        self.fonts.width(text, style)
    }

    /// The design tokens.
    #[must_use]
    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// The form an intent names.
    ///
    /// # Errors
    ///
    /// An unknown form, refused with the list of forms (ADR-003).
    pub fn form(&self, name: &str) -> Result<&Form, IntentError> {
        self.forms.iter().find(|f| f.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.forms.iter().map(|f| f.name.as_str()).collect();
            IntentError::new(
                "form",
                format!("\"{name}\" is not a form"),
                format!("use one of: {}", names.join(", ")),
            )
        })
    }

    /// Corrects what `intent` asks of its form wherever the card then shows
    /// no wrong number, noting each change in `intent.notes` (ADR-020); an
    /// unknown form is left for [`Engine::scene`] to refuse.
    pub fn mend(&self, intent: &mut Intent) {
        if let Ok(form) = self.form(&intent.form) {
            let pal = self.tokens.themes.default_palette();
            layout::mend(form, intent, pal);
        }
    }

    /// Lays out an intent.
    ///
    /// # Errors
    ///
    /// When the intent breaks a form rule, or the options are out of range.
    pub fn scene(&self, intent: &Intent, opts: &Options) -> Result<Scene, RenderError> {
        self.scene_aligned(intent, opts, Align::default())
    }

    /// Lays out an intent as one card of a row: [`Align`] says how it lines
    /// up with the others.
    ///
    /// # Errors
    ///
    /// As [`Engine::scene`].
    pub fn scene_aligned(
        &self,
        intent: &Intent,
        opts: &Options,
        align: Align,
    ) -> Result<Scene, RenderError> {
        if !(MIN_WIDTH..=MAX_WIDTH).contains(&opts.width) {
            return Err(RenderError::Engine(format!(
                "width {} is outside {MIN_WIDTH} to {MAX_WIDTH}",
                opts.width
            )));
        }
        if opts
            .cell_height
            .is_some_and(|c| !(c.is_finite() && (1.0..=200.0).contains(&c)))
        {
            return Err(RenderError::Engine(
                "cell height must be between 1 and 200".into(),
            ));
        }
        let form = self.form(&intent.form)?;
        Ok(layout::scene(
            &self.tokens,
            &self.fonts,
            form,
            intent,
            opts,
            align,
        )?)
    }

    /// Lays out a dashboard's heading, `opts.width` wide: its title and an
    /// optional shared note, as text with no card box, for the top of a
    /// [`Board`].
    #[must_use]
    pub fn heading(&self, title: &str, note: Option<&str>, opts: &Options) -> Scene {
        layout::heading(&self.tokens, &self.fonts, title, note, opts)
    }

    /// When card `k` of a board of `tiles` starts its entrance, in
    /// milliseconds after the first: 50 ms apart, closer when there are
    /// many, so the last starts at most 200 ms after the first, always on a
    /// whole frame so a wrapper tick shows each start.
    #[must_use]
    pub fn start_ms(&self, k: usize, tiles: usize) -> f32 {
        #[allow(clippy::cast_precision_loss)] // Card counts.
        let (k, gaps) = (k as f32, tiles.saturating_sub(1) as f32);
        if gaps == 0.0 {
            return 0.0;
        }
        let frame = self.tokens.motion.frame_ms;
        let gap = STAGGER_MS.min(MAX_STAGGER_MS / gaps);
        (k * gap / frame + 1e-3).floor() * frame
    }

    /// How long a board's `entrance` runs: one card's, after the last
    /// card's start ([`Engine::start_ms`]); the reduced entrance fades
    /// every card at once.
    #[must_use]
    pub fn board_entrance_ms(&self, board: &Board, entrance: Entrance) -> f32 {
        let n = board.tiles.len();
        match entrance {
            Entrance::Full => {
                self.tokens.motion.duration_ms + self.start_ms(n.saturating_sub(1), n)
            }
            Entrance::Reduced => self.tokens.motion.reduce_ms,
        }
    }

    /// Frames, the settled picture included, of a board's `entrance` drawn
    /// `frame_ms` apart ([`motion::frame_count`]).
    #[must_use]
    pub fn board_frame_count(&self, board: &Board, entrance: Entrance) -> usize {
        motion::frame_count(
            self.board_entrance_ms(board, entrance),
            self.tokens.motion.frame_ms,
        )
    }

    /// Draws the settled card as PNG bytes.
    ///
    /// # Errors
    ///
    /// When the scale is out of range or Skia cannot draw or encode.
    pub fn png(&self, scene: &Scene, scale: f32) -> Result<Vec<u8>, RenderError> {
        check_scale(scale)?;
        raster::Prepared::new(scene, &self.tokens, &self.fonts)
            .png(scale)
            .ok_or_else(|| RenderError::Engine("Skia could not draw or encode the card".into()))
    }

    /// Draws several settled cards on one picture as PNG bytes.
    ///
    /// # Errors
    ///
    /// When the scale is out of range, the board is empty or out of size, or
    /// Skia cannot draw or encode.
    pub fn board_png(&self, board: &Board, scale: f32) -> Result<Vec<u8>, RenderError> {
        check_scale(scale)?;
        check_board(board)?;
        raster::board_png(board, &self.tokens, &self.fonts, scale)
            .ok_or_else(|| RenderError::Engine("Skia could not draw or encode the board".into()))
    }

    /// Draws a board's `entrance` as frames `frame_ms` apart over
    /// [`Engine::board_entrance_ms`]: the cards enter one after another in
    /// the board's order (row by row, left to right), each from its
    /// [`Engine::start_ms`]. It returns the PNG of every frame but the last
    /// ([`Engine::board_frame_count`]), which is the settled picture
    /// [`Engine::board_png`] draws. Frames are encoded fast (zlib level 1),
    /// since they are drawn while a tool call waits.
    ///
    /// # Errors
    ///
    /// As [`Engine::board_png`].
    pub fn board_entrance(
        &self,
        board: &Board,
        scale: f32,
        entrance: Entrance,
    ) -> Result<Vec<Vec<u8>>, RenderError> {
        check_scale(scale)?;
        check_board(board)?;
        let motion = &self.tokens.motion;
        let count = self.board_frame_count(board, entrance);
        let n = board.tiles.len();
        let prepared = raster::PreparedBoard::new(board, &self.tokens, &self.fonts);
        (0..count - 1)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // A frame index.
                let at = motion.frame_ms * i as f32;
                let moments: Vec<Moment> = (0..n)
                    .map(|k| {
                        let start = match entrance {
                            Entrance::Full => self.start_ms(k, n),
                            Entrance::Reduced => 0.0,
                        };
                        entrance.at(motion, at - start)
                    })
                    .collect();
                prepared
                    .png_at(scale, &moments, Some(1))
                    .ok_or_else(|| RenderError::Engine("Skia could not draw a frame".into()))
            })
            .collect()
    }

    /// Draws the entrance (playbook N1): one frame per moment of
    /// [`Moment::frames`], the last being the settled card. Each frame is
    /// drawn as the iterator reaches it, so only one is in memory at a time.
    ///
    /// # Errors
    ///
    /// When the scale is out of range, or for a frame Skia cannot draw.
    pub fn frames<'a>(
        &'a self,
        scene: &'a Scene,
        scale: f32,
    ) -> Result<impl Iterator<Item = Result<Frame, RenderError>> + 'a, RenderError> {
        check_scale(scale)?;
        let prepared = raster::Prepared::new(scene, &self.tokens, &self.fonts);
        let (width, height) = prepared.size(scale);
        Ok(Moment::frames(&self.tokens.motion, Entrance::Full)
            .into_iter()
            .map(move |m| {
                prepared
                    .rgba(scale, m)
                    .map(|rgba| Frame {
                        width,
                        height,
                        rgba,
                    })
                    .ok_or_else(|| RenderError::Engine("Skia could not draw a frame".into()))
            }))
    }

    /// Draws one moment of the entrance as unpremultiplied RGBA.
    ///
    /// # Errors
    ///
    /// When the scale is out of range or Skia cannot draw.
    pub fn frame_at(
        &self,
        scene: &Scene,
        scale: f32,
        moment: Moment,
    ) -> Result<Frame, RenderError> {
        check_scale(scale)?;
        let prepared = raster::Prepared::new(scene, &self.tokens, &self.fonts);
        let (width, height) = prepared.size(scale);
        prepared
            .rgba(scale, moment)
            .map(|rgba| Frame {
                width,
                height,
                rgba,
            })
            .ok_or_else(|| RenderError::Engine("Skia could not draw a frame".into()))
    }

    /// Parses, lays out and draws an intent in one call.
    ///
    /// # Errors
    ///
    /// Any [`RenderError`].
    pub fn render_json(
        &self,
        json: &str,
        opts: &Options,
        scale: f32,
    ) -> Result<Vec<u8>, RenderError> {
        let intent = tiles_intent::parse(json)?;
        let scene = self.scene(&intent, opts)?;
        self.png(&scene, scale)
    }
}

fn check_scale(scale: f32) -> Result<(), RenderError> {
    if scale.is_finite() && (0.5..=MAX_SCALE).contains(&scale) {
        Ok(())
    } else {
        Err(RenderError::Engine(format!(
            "scale {scale} is outside 0.5 to {MAX_SCALE}"
        )))
    }
}

fn check_board(board: &Board) -> Result<(), RenderError> {
    let fits = |v: f32| v.is_finite() && (1.0..=MAX_BOARD).contains(&v);
    if board.tiles.is_empty() || !fits(board.width) || !fits(board.height) {
        return Err(RenderError::Engine(format!(
            "a board of {} cards, {} by {} px, cannot be drawn",
            board.tiles.len(),
            board.width,
            board.height
        )));
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn dev_dir() -> Option<std::path::PathBuf> {
    std::env::var_os(DEV_ASSETS_VAR).map(std::path::PathBuf::from)
}

#[cfg(not(debug_assertions))]
fn dev_dir() -> Option<std::path::PathBuf> {
    None
}

/// Encodes a frame as PNG bytes.
#[must_use]
pub fn frame_png(frame: &Frame) -> Option<Vec<u8>> {
    let info = skia_safe::ImageInfo::new(
        (frame.width, frame.height),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let row = usize::try_from(frame.width).ok()? * 4;
    let image =
        skia_safe::images::raster_from_data(&info, skia_safe::Data::new_copy(&frame.rgba), row)?;
    let data = image.encode(None, skia_safe::EncodedImageFormat::PNG, None)?;
    Some(data.as_bytes().to_vec())
}

/// Draws a blank card-coloured raster and returns its PNG bytes.
///
/// Returns `None` if Skia cannot allocate the surface or encode the image.
#[must_use]
pub fn blank_png(width: i32, height: i32) -> Option<Vec<u8>> {
    let mut surface = skia_safe::surfaces::raster_n32_premul((width, height))?;
    surface.canvas().clear(skia_safe::Color::WHITE);
    let image = surface.image_snapshot();
    let data = image.encode(None, skia_safe::EncodedImageFormat::PNG, None)?;
    Some(data.as_bytes().to_vec())
}

#[cfg(test)]
mod tests;
