//! One chart request in, one saved card out: parse the intent, fetch its
//! rows when it names an Omni topic or a warehouse query, draw, save, and
//! describe the card.
//!
//! The engine sits behind [`Painter`], the narrow interface every surface
//! uses, so the server's tests run without Skia and the engine's internals can
//! change without touching a surface. The data sources sit behind [`Fetch`],
//! injected by the binary, so this crate never reaches the network (ADR-005).

mod draw;
mod painter;
mod redraw;
mod request;
mod result;
mod save;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;
use std::time::Instant;

pub use tiles_engine::forms::Fit;
use tiles_intent::schema::FormEntry;
use tiles_intent::source::{OmniSource, Table, WarehouseSource};
use tiles_intent::{Intent, IntentError, Request};

use crate::cache::Cache;

pub use tiles_engine::Theme;

pub use painter::EnginePainter;
pub use redraw::{
    Rethemed, Shown, needs_its_session, needs_its_session_note, redraw, reopen, retheme, shown,
};
pub use result::text_result;

use draw::draw;
use save::save;

/// The terminal geometry a card is fitted to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// The theme.
    pub theme: Theme,
    /// The terminal's width in columns.
    pub term_cols: u16,
    /// The terminal's height in rows, where the route shows only a card's
    /// visible rows: a taller card is drawn smaller ([`Look::card_rows`]).
    pub term_rows: Option<u16>,
    /// One cell in device pixels: width, height.
    pub cell_px: (f32, f32),
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Draws the entrance beside the card when 2 or more: `tiles run` sets
    /// it to the frames of one card's entrance wherever the wrapper plays
    /// it (kitty and the pre-cut route). The frames are drawn `frame_ms`
    /// apart for as long as the entrance runs, so a dashboard's, staggered,
    /// has more. 0 draws none.
    pub frames: u16,
    /// Which entrance: the reduced one under reduce motion
    /// (`TILES_MOTION=reduce`, playbook N2).
    pub entrance: tiles_engine::Entrance,
    /// The `tiles run` session the card is made for, written into its
    /// geometry so only that session's wrapper draws it and sends it ahead.
    pub session: Option<u64>,
}

impl Look {
    /// The `tiles run` session whose cards a theme change draws again in
    /// place: only on kitty's placeholder route, where the wrapper plays
    /// entrances ([`Look::frames`]) and gets no screen height
    /// ([`Look::term_rows`] is the pre-cut route's), the one route that
    /// swaps a picture already on screen (ADR-021).
    #[must_use]
    pub fn swapping_session(&self) -> Option<u64> {
        self.session
            .filter(|_| self.frames > 0 && self.term_rows.is_none())
    }
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            term_cols: 100,
            term_rows: None,
            cell_px: (16.0, 32.0),
            scale: 2.0,
            frames: 0,
            entrance: tiles_engine::Entrance::Full,
            session: None,
        }
    }
}

/// Rows a card leaves under it for the harness's input box and the
/// reply's first lines ([`Look::card_rows`]).
pub const REPLY_ROWS: u16 = 10;

/// Narrowest and widest card, in columns.
pub const CARD_COLS: (u16, u16) = (30, 72);
/// Most cards in one row.
pub const MAX_CARDS: usize = 4;
/// Most rows in a dashboard.
pub const MAX_ROWS: usize = 4;
/// Most cards one call draws on one picture.
pub const MAX_TILES: usize = 8;

impl Look {
    /// Columns the terminal leaves for cards: its width less a margin.
    #[must_use]
    pub fn room(&self) -> u16 {
        self.term_cols.saturating_sub(8)
    }

    /// The narrowest card in columns: the floor of [`CARD_COLS`], or the
    /// engine's narrowest tile when that is wider.
    #[must_use]
    pub fn min_cols(&self) -> u16 {
        let engine =
            f64::from(tiles_engine::MIN_WIDTH) * f64::from(self.scale) / f64::from(self.cell_px.0);
        let engine =
            u16::try_from(whole((engine - 0.01).ceil(), u32::from(u16::MAX))).unwrap_or(u16::MAX);
        engine.max(CARD_COLS.0)
    }

    /// A card's width in columns: what its form and rows ask for, within the
    /// terminal's room and [`CARD_COLS`].
    #[must_use]
    pub fn card_cols(&self, fit: Fit) -> u16 {
        fit.ideal
            .min(self.room())
            .min(CARD_COLS.1)
            .max(self.min_cols())
    }

    /// A width in columns as CSS pixels.
    #[must_use]
    pub fn width_css(&self, cols: u16) -> f32 {
        f32::from(cols) * self.cell_px.0 / self.scale
    }

    /// One cell's height in CSS pixels.
    #[must_use]
    pub fn cell_height_css(&self) -> f32 {
        self.cell_px.1 / self.scale
    }

    /// The most rows a card may take: the terminal's rows less
    /// [`REPLY_ROWS`] for the harness's input box and the reply's first
    /// lines, or half the rows in a short terminal. `None` when the terminal's
    /// rows are not known.
    #[must_use]
    pub fn card_rows(&self) -> Option<u16> {
        self.term_rows
            .map(|r| r.saturating_sub(REPLY_ROWS).max(r / 2).max(1))
    }
}

/// One card to draw: its intent, its width in columns, and the key path its
/// refusals name (empty for the call's own intent, `row[1]` for a card under
/// `row`).
#[derive(Debug, Clone, Copy)]
pub struct Tile<'a> {
    /// The intent.
    pub intent: &'a Intent,
    /// Width in columns.
    pub cols: u16,
    /// Where the intent sits in the call's arguments.
    pub key: &'a str,
}

/// A dashboard's heading: its title and the note its cards share (data
/// source, date range), drawn once above the cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// The dashboard's title.
    pub title: String,
    /// The shared note.
    pub note: Option<String>,
}

/// What one card on a picture shows, for the result text.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// The headline.
    pub title: String,
    /// The form drawn.
    pub form: String,
    /// One line per tooltip: the values the card shows.
    pub values: Vec<String>,
    /// What the engine left out of the intent rather than refusing it, one
    /// sentence each.
    pub notes: Vec<String>,
    /// Where its rows came from (`Omni`, `BigQuery`) when Tiles fetched
    /// them; `None` when the model sent them.
    pub fetched: Option<String>,
}

/// A drawn picture before it is saved.
#[derive(Debug, Clone, PartialEq)]
pub struct Painted {
    /// PNG bytes.
    pub png: Vec<u8>,
    /// Pixel size.
    pub width_px: u32,
    /// Pixel size.
    pub height_px: u32,
    /// Columns and rows the picture covers.
    pub cells: (u16, u16),
    /// Each card, in the order drawn: row by row, left to right.
    pub faces: Vec<Face>,
    /// The entrance as PNGs at half the picture's resolution, ending with
    /// the settled picture at that resolution; empty when [`Look::frames`]
    /// is 0.
    pub frames: Vec<Vec<u8>>,
    /// Milliseconds from one entrance frame to the next.
    pub frame_ms: u32,
}

/// The narrow interface to the engine.
pub trait Painter {
    /// The forms, for the schema and the tool description.
    fn forms(&self) -> Vec<FormEntry>;
    /// How wide a card of the intent's form wants to be, alone or (`among`)
    /// as one of several cards: a scorecard among others is compact and
    /// asks only for its form's narrowest width.
    ///
    /// # Errors
    ///
    /// The intent names no form.
    fn fit(&self, intent: &Intent, among: bool) -> Result<Fit, IntentError>;
    /// Corrects what the intent asks of its form where the card then shows
    /// no wrong number, noting each change in `intent.notes`.
    fn mend(&self, _intent: &mut Intent) {}
    /// Whether the intent's form is a scorecard (one headline number),
    /// which a dashboard puts in its top rows.
    fn scorecard(&self, _intent: &Intent) -> bool {
        false
    }
    /// Draws validated intents as one picture: the heading, when there is
    /// one, then rows top to bottom, each row's cards left to right, placed
    /// by [`board::place`](crate::board::place).
    ///
    /// # Errors
    ///
    /// An intent breaks a form rule (a refusal the model can fix), or the
    /// engine failed.
    fn paint(
        &self,
        rows: &[Vec<Tile<'_>>],
        heading: Option<&Heading>,
        look: &Look,
    ) -> Result<Painted, Refusal>;
}

/// The query an intent's rows come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query<'a> {
    /// An Omni topic (ADR-008).
    Omni(&'a OmniSource),
    /// A warehouse `SELECT` (ADR-031).
    Warehouse(&'a WarehouseSource),
}

/// Rows a data source fetched for one card.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    /// The rows.
    pub table: Table,
    /// Where they came from, as a chart result names it: `Omni`, `BigQuery`.
    pub origin: String,
    /// What the user should know about the fetch, one sentence each, with
    /// no row value in any: the build a model was read from, the bytes the
    /// query scanned, a cut. They join the card's notes.
    pub notes: Vec<String>,
    /// Where the rows were read from, in a few words for the card itself,
    /// such as `dev build · personal_ada.fct_payments · built 2 h ago`: the
    /// subtitle of a lone card that has none, and the note of a dashboard
    /// that has none when every fetched card shares it, so a card read from
    /// the wrong build shows it.
    pub label: Option<String>,
}

impl Fetched {
    /// Rows fetched from Omni, with nothing to note.
    #[must_use]
    pub fn omni(table: Table) -> Self {
        Self {
            table,
            origin: "Omni".to_string(),
            notes: Vec::new(),
            label: None,
        }
    }
}

/// Fetches the rows a query names. The [`Instant`] is when the tool call
/// began, so the cards of one call share one time budget.
pub type Fetch<'a> = dyn Fn(Query<'_>, Instant) -> Result<Fetched, String> + 'a;

/// Why no card was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The intent breaks a rule; the model can fix and retry.
    Intent(IntentError),
    /// Omni refused or failed; the message carries no secret (ADR-008).
    Omni(String),
    /// The warehouse refused or failed; the message carries no secret and
    /// no row value (ADR-031).
    Warehouse(String),
    /// The engine or the cache failed.
    Internal(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Intent(e) => write!(f, "{e}"),
            Refusal::Omni(e) => write!(f, "omni: {e}"),
            Refusal::Warehouse(e) => write!(f, "warehouse: {e}"),
            Refusal::Internal(e) => write!(f, "tiles: {e}"),
        }
    }
}

/// An intent error under `key`, so a refusal names the card in a row; one
/// about a card as a whole names the card alone.
#[must_use]
pub fn within(key: &str, e: IntentError) -> IntentError {
    if key.is_empty() {
        e
    } else if e.key == tiles_intent::ARGUMENTS {
        IntentError {
            key: key.to_string(),
            ..e
        }
    } else {
        IntentError {
            key: format!("{key}.{}", e.key),
            ..e
        }
    }
}

/// A saved picture: one card, or several cards drawn together.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    /// Six hex digits, the id the placeholder block and the add-ons use.
    pub id: String,
    /// The saved PNG.
    pub path: PathBuf,
    /// PNG bytes.
    pub png: Vec<u8>,
    /// The block's headline: the card's title, or every card's joined.
    pub title: String,
    /// Columns and rows the picture covers.
    pub cells: (u16, u16),
    /// Pixel size.
    pub size_px: (u32, u32),
    /// Each card on the picture.
    pub faces: Vec<Face>,
    /// The dashboard's title, when the call gave one.
    pub heading: Option<String>,
    /// Lines of cards on the picture.
    pub lines: usize,
    /// Whether cards asked for side by side went on more lines because a
    /// row did not fit, and the sentences that say so.
    pub stacked: Option<String>,
}

impl Card {
    /// The forms drawn, joined.
    #[must_use]
    pub fn forms(&self) -> String {
        let forms: Vec<&str> = self.faces.iter().map(|f| f.form.as_str()).collect();
        forms.join(", ")
    }
}

/// A pixel or cell count from a finite float: rounded, and clamped to
/// `0..=max` so no cast can wrap.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to the target's range first.
fn whole(x: f64, max: u32) -> u32 {
    if x.is_finite() {
        x.round().clamp(0.0, f64::from(max)) as u32
    } else {
        0
    }
}

/// What one call asks for: rows of cards, each card with the key path its
/// refusals name, and the dashboard's heading.
struct Asked {
    rows: Vec<Vec<(String, Request)>>,
    heading: Option<Heading>,
}

/// A picture drawn from one call, before it is saved.
struct Drawn {
    art: Painted,
    stacked: Option<String>,
    /// The rows fetched for each card, in call order; `None` for a card
    /// whose rows the model sent.
    tables: Vec<Option<Fetched>>,
    /// The dashboard's title, when the call asked for one.
    heading: Option<String>,
    /// Lines the cards went on.
    lines: usize,
}

/// The cells a picture covers, at least one by one and at most
/// [`tiles_term::block::MAX_ROWS`] tall.
fn cells_of(art: &Painted) -> (u16, u16) {
    (
        art.cells.0.max(1),
        art.cells.1.clamp(1, tiles_term::block::MAX_ROWS),
    )
}

/// Makes a card from the tool's arguments: parse, fetch, size, draw, save.
/// The call and the geometry are saved beside the card and the rows fetched
/// from Omni or a warehouse stay in this process's memory ([`Cache::hold`]),
/// so [`retheme`] can draw it again in another theme without fetching twice
/// (ADR-027).
///
/// # Errors
///
/// A [`Refusal`] naming what went wrong.
pub fn make(
    json: &str,
    painter: &dyn Painter,
    fetch: &Fetch<'_>,
    cache: &Cache,
    look: &Look,
) -> Result<Card, Refusal> {
    let drawn = draw(json, painter, fetch, look)?;
    let call = serde_json::from_str::<serde_json::Value>(json).unwrap_or_default();
    save(&call, drawn, cache, look)
}
