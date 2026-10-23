//! Drawing a saved card again: in a new theme, in place or as a new card.

use std::path::PathBuf;
use std::time::Instant;

use super::draw::draw;
use super::save::save;
use super::{Card, Drawn, Fetched, Look, Painter, Query, Theme, cells_of};
use crate::cache::Cache;

/// Where each card of saved call `saved` got its rows: the origin Tiles
/// fetched them from, or `None` for rows the model sent. A call file
/// written before the warehouse says `omni`, and one written before
/// ADR-027 says so by the rows it holds.
fn origins(saved: &serde_json::Value) -> Vec<Option<String>> {
    let list = |k: &str| saved.get(k).and_then(serde_json::Value::as_array);
    if let Some(fetched) = list("fetched") {
        return fetched
            .iter()
            .map(|v| v.as_str().map(|o| tiles_term::sanitize::line(o, 40)))
            .collect();
    }
    let omni = |hit: fn(&serde_json::Value) -> bool, k: &str| {
        list(k).map(|a| {
            a.iter()
                .map(|v| hit(v).then(|| "Omni".to_string()))
                .collect()
        })
    };
    omni(|v| v.as_bool() == Some(true), "omni")
        .or_else(|| omni(|v| !v.is_null(), "tables"))
        .unwrap_or_default()
}

/// Whether saved call `saved` drew a card from fetched rows.
fn from_fetched(saved: &serde_json::Value) -> bool {
    origins(saved).iter().any(Option::is_some)
}

/// Whether card `id` was drawn from fetched rows this process does not
/// hold, so only the session that fetched them can draw it again (ADR-027).
#[must_use]
pub fn needs_its_session(cache: &Cache, id: &str) -> bool {
    tiles_term::block::is_card_id(id)
        && !cache.holds(id)
        && cache.read_call(id).is_some_and(|c| from_fetched(&c))
}

/// The line that says card `id`, drawn from rows fetched from `origin`, can
/// be drawn again only by the session that fetched them, for a user who
/// asked for `theme`.
#[must_use]
pub fn needs_its_session_note(id: &str, origin: &str, theme: Theme) -> String {
    format!(
        "Card {} was drawn from {} rows, which Tiles keeps only in the memory of the session \
         that fetched them, so only that session can draw it again in another theme; ask for \
         the chart again to see it in {}.",
        tiles_term::sanitize::line(id, 6),
        tiles_term::sanitize::line(origin, 40),
        theme.name()
    )
}

/// The origin card `id`'s rows were first fetched from, for
/// [`needs_its_session_note`].
fn origin_of(cache: &Cache, id: &str) -> String {
    cache
        .read_call(id)
        .and_then(|c| origins(&c).into_iter().flatten().next())
        .unwrap_or_else(|| "fetched".to_string())
}

/// Saved card `id`'s call, drawn again with `look` (`None`: the geometry
/// it was first drawn at, with `theme`) from the rows this process fetched
/// for it the first time, so Omni is never asked twice.
fn draw_saved(
    id: &str,
    painter: &dyn Painter,
    cache: &Cache,
    look: Option<&Look>,
    theme: Theme,
) -> Result<(serde_json::Value, Drawn, Look), String> {
    if !tiles_term::block::is_card_id(id) {
        return Err(format!(
            "\"{}\" is not a card id (six hex digits)",
            tiles_term::sanitize::line(id, 40)
        ));
    }
    let saved = cache.read_call(id).ok_or_else(|| {
        format!("no card {id} in the cache, or it was drawn before Tiles kept calls")
    })?;
    let num = |v: &serde_json::Value, k: &str| v.get(k).and_then(serde_json::Value::as_f64);
    let geometry = &saved["look"];
    let cell_px = geometry
        .get("cell_px")
        .and_then(serde_json::Value::as_array);
    let px = |i: usize| {
        cell_px
            .and_then(|c| c.get(i))
            .and_then(serde_json::Value::as_f64)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Checked ranges, written from the u16 and f32 they came from.
    let first = || -> Option<Look> {
        let cols = num(geometry, "term_cols").filter(|c| (10.0..=1000.0).contains(c))?;
        let (w, h) = (px(0)?, px(1)?);
        let scale = num(geometry, "scale").filter(|s| (0.5..=4.0).contains(s))?;
        ((2.0..=200.0).contains(&w) && (2.0..=400.0).contains(&h)).then_some(Look {
            theme,
            term_cols: cols as u16,
            term_rows: num(geometry, "term_rows")
                .filter(|r| (2.0..=1000.0).contains(r))
                .map(|r| r as u16),
            cell_px: (w as f32, h as f32),
            scale: scale as f32,
            frames: 0,
            entrance: tiles_engine::Entrance::Full,
            session: None,
        })
    };
    let look = match look {
        Some(l) => *l,
        None => first().ok_or_else(|| format!("card {id} was saved without its geometry"))?,
    };
    let tables = cache.held(id).unwrap_or_default();
    let from = origins(&saved);
    if tables.is_empty() && from.iter().any(Option::is_some) {
        return Err(needs_its_session_note(id, &origin_of(cache, id), theme));
    }
    let labels: Vec<Option<String>> = saved
        .get("labels")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let held = std::cell::RefCell::new(
        tables
            .into_iter()
            .zip(from.into_iter().chain(std::iter::repeat(None)))
            .zip(labels.into_iter().chain(std::iter::repeat(None)))
            .filter_map(|((t, o), l)| Some((t?, o, l))),
    );
    let fetch = |_: Query<'_>, _: Instant| {
        held.borrow_mut()
            .next()
            .map(|(table, origin, label)| Fetched {
                table,
                origin: origin.unwrap_or_else(|| "Omni".to_string()),
                notes: Vec::new(),
                label,
            })
            .ok_or_else(|| "the rows fetched for this card are no longer held".to_string())
    };
    let call = saved["call"].clone();
    let drawn = draw(&call.to_string(), painter, &fetch, &look).map_err(|r| r.to_string())?;
    Ok((call, drawn, look))
}

/// Draws saved card `id` again as a new card with `look`, in its theme:
/// `tiles open <id>`, for a terminal where an earlier card cannot change in
/// place.
///
/// # Errors
///
/// When there is no such card, it was saved without its call, or it no
/// longer draws.
pub fn reopen(id: &str, painter: &dyn Painter, cache: &Cache, look: &Look) -> Result<Card, String> {
    let (call, drawn, _) = draw_saved(id, painter, cache, Some(look), look.theme)?;
    save(&call, drawn, cache, look).map_err(|r| r.to_string())
}

/// A saved card's picture, for a window of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The card's id: the one asked for, or the new card drawn for `theme`.
    pub id: String,
    /// Its PNG.
    pub path: PathBuf,
    /// The dashboard's title, or the card titles joined.
    pub title: String,
    /// The theme the picture is in.
    pub theme: Theme,
    /// Why the picture is not in the theme asked for: a card drawn from
    /// fetched rows this process does not hold ([`needs_its_session_note`]).
    pub note: Option<String>,
}

/// Saved card `id` in `theme` as it was drawn, for `tiles open` to show in
/// a window of its own (ADR-023): its own PNG when it was drawn in `theme`,
/// else its call drawn again in `theme` at the geometry it was first drawn
/// at, so a dashboard keeps its layout, and saved as a new card. A card
/// drawn from fetched rows this process does not hold is shown as it was
/// drawn, with a note saying why (ADR-027).
///
/// # Errors
///
/// When there is no such card, or it must be drawn again and was saved
/// without its call or no longer draws.
pub fn shown(
    id: &str,
    painter: &dyn Painter,
    cache: &Cache,
    theme: Theme,
) -> Result<Shown, String> {
    if !tiles_term::block::is_card_id(id) {
        return Err(format!(
            "\"{}\" is not a card id (six hex digits)",
            tiles_term::sanitize::line(id, 40)
        ));
    }
    let meta = cache
        .read_meta(id)
        .ok_or_else(|| format!("no card {id} in the cache"))?;
    let path = cache.card_path(id);
    let drawn_in = meta
        .get("theme")
        .and_then(serde_json::Value::as_str)
        .and_then(Theme::from_name);
    let kept = drawn_in.filter(|t| *t == theme || needs_its_session(cache, id));
    if let Some(drawn_in) = kept
        && path.is_file()
    {
        let title = meta
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        return Ok(Shown {
            id: id.to_string(),
            path,
            title,
            theme: drawn_in,
            note: (drawn_in != theme)
                .then(|| needs_its_session_note(id, &origin_of(cache, id), theme)),
        });
    }
    let (call, drawn, look) = draw_saved(id, painter, cache, None, theme)?;
    let card = save(&call, drawn, cache, &look).map_err(|r| r.to_string())?;
    Ok(Shown {
        id: card.id,
        path: card.path,
        title: card.title,
        theme,
        note: None,
    })
}

/// Draws saved card `id` again in `theme`, under the same id: its call
/// again, with the rows this process fetched for it the first time and the
/// geometry it was drawn at, so it covers the same cells. The picture is written first
/// and the geometry last, with no entrance, so the wrapper sends the new
/// picture under the card's kitty image id and every copy on screen and in
/// scrollback changes in place.
///
/// # Errors
///
/// When the card was saved without its call, was drawn from fetched rows this
/// process does not hold, or its call no longer draws the same size.
pub fn redraw(id: &str, painter: &dyn Painter, cache: &Cache, theme: Theme) -> Result<(), String> {
    let meta = cache
        .read_meta(id)
        .ok_or_else(|| format!("card {id} has no geometry"))?;
    let (_, drawn, _) = draw_saved(id, painter, cache, None, theme)?;
    let cells = cells_of(&drawn.art);
    let num = |k: &str| meta.get(k).and_then(serde_json::Value::as_u64);
    if (num("cols"), num("rows")) != (Some(u64::from(cells.0)), Some(u64::from(cells.1))) {
        return Err(format!("card {id} would change size in {}", theme.name()));
    }
    cache.write_card(id, &drawn.art.png)?;
    cache.remove_frames(id);
    let mut meta = meta;
    if let Some(m) = meta.as_object_mut() {
        m.insert("frames".into(), 0.into());
        m.insert("theme".into(), theme.name().into());
    }
    cache.write_meta(id, &meta)
}

/// What [`retheme`] did to a session's cards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rethemed {
    /// Cards drawn again in the new theme.
    pub done: usize,
    /// Cards that could not be drawn again.
    pub failed: usize,
    /// Cards whose rows another process fetched and holds, so
    /// they keep their theme (ADR-027).
    pub kept: usize,
}

/// Draws every card the `tiles run` session `session` made again in
/// `theme`, oldest first.
#[must_use]
pub fn retheme(painter: &dyn Painter, cache: &Cache, session: u64, theme: Theme) -> Rethemed {
    let mut out = Rethemed::default();
    for id in cache.session_cards(session) {
        if cache.read_meta(&id).is_some_and(|m| {
            m.get("theme").and_then(serde_json::Value::as_str) == Some(theme.name())
        }) {
            continue;
        }
        if needs_its_session(cache, &id) {
            out.kept += 1;
            continue;
        }
        match redraw(&id, painter, cache, theme) {
            Ok(()) => out.done += 1,
            Err(_) => out.failed += 1,
        }
    }
    out
}
