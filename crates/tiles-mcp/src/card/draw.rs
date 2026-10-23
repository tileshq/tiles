//! Drawing a call as one picture, fitted to the terminal.

use tiles_intent::IntentError;

use super::request::{asks, label, load, requests};
use super::{Drawn, Fetch, Heading, Look, Painted, Painter, Refusal, Tile, whole};
use crate::board;

/// The sentence for each asked row that went on more lines than one.
fn split_note(split: &[board::Split], rows: usize, term_cols: u16) -> Option<String> {
    let notes: Vec<String> = split
        .iter()
        .map(|s| {
            let which = if rows > 1 {
                format!("Row {}'s {} cards", s.row + 1, s.cards)
            } else {
                format!("The {} cards", s.cards)
            };
            let how = if s.lines == s.cards {
                "stacked them one above the other".to_string()
            } else {
                format!("put them on {} lines", s.lines)
            };
            format!(
                "{which} need at least {} columns side by side and this terminal has \
                 {term_cols}, so Tiles {how}.",
                s.need
            )
        })
        .collect();
    (!notes.is_empty()).then(|| notes.join(" "))
}

/// Parses the call, fetches its rows, sizes and draws it. Several
/// cards under `row` and `rows` come out as one picture, packed to the
/// terminal's width ([`board::plan`]), with the `dashboard` heading above
/// them.
pub(super) fn draw(
    json: &str,
    painter: &dyn Painter,
    fetch: &Fetch<'_>,
    look: &Look,
) -> Result<Drawn, Refusal> {
    let mut asked = requests(json).map_err(Refusal::Intent)?;
    let shape: Vec<usize> = asked.rows.iter().map(Vec::len).collect();
    let rows = std::mem::take(&mut asked.rows);
    let (keys, mut intents, tables) = load(rows, painter, fetch)?;
    label(&mut intents, asked.heading.as_mut(), &tables);
    let among = intents.len() > 1;
    let asks = asks(&intents, &keys, &shape, painter)?;
    let plan = board::plan(&asks, look);
    let rows: Vec<Vec<Tile<'_>>> = plan
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(i, cols)| Tile {
                    intent: &intents[*i],
                    cols: *cols,
                    key: &keys[*i],
                })
                .collect()
        })
        .collect();
    let mut art = painter.paint(&rows, asked.heading.as_ref(), look)?;
    if let Some(most) = look.card_rows() {
        art = fit_rows(art, &rows, asked.heading.as_ref(), painter, look, most)?;
    }
    let order: Vec<usize> = plan.rows.iter().flatten().map(|(i, _)| *i).collect();
    for (face, i) in art.faces.iter_mut().zip(&order) {
        face.fetched = tables[*i].as_ref().map(|f| f.origin.clone());
    }
    if (among || asked.heading.is_some()) && art.cells.1 > tiles_term::block::MAX_ROWS {
        return Err(Refusal::Intent(IntentError::new(
            if shape.len() > 1 { "rows" } else { "row" },
            format!(
                "the cards stack {} rows tall in this terminal, over {}",
                art.cells.1,
                tiles_term::block::MAX_ROWS
            ),
            "draw fewer cards in this call",
        )));
    }
    let mut stacked = split_note(
        &plan.split,
        if plan.moved { 2 } else { shape.len() },
        look.term_cols,
    );
    if plan.moved {
        let note = "Tiles put the scorecards in a row of their own above the charts, so each \
                    card keeps its own height."
            .to_string();
        stacked = Some(stacked.map_or_else(|| note.clone(), |s| format!("{note} {s}")));
    }
    Ok(Drawn {
        art,
        stacked,
        tables,
        heading: asked.heading.map(|h| h.title),
        lines: plan.rows.len(),
    })
}

/// `art` drawn smaller when it is more than `most` rows tall: the same
/// layout over fewer cells, as if each cell were larger, so the terminal
/// shows it whole. The pixels stay as sharp; only the cells change.
fn fit_rows(
    art: Painted,
    rows: &[Vec<Tile<'_>>],
    heading: Option<&Heading>,
    painter: &dyn Painter,
    look: &Look,
    most: u16,
) -> Result<Painted, Refusal> {
    let mut art = art;
    let mut zoom = 1.0_f32;
    // The engine snaps a height up to whole cells, so a first try can come
    // out a row over; a few more always settle it.
    for _ in 0..4 {
        if art.cells.1 <= most {
            break;
        }
        zoom *= f32::from(art.cells.1) / f32::from(most);
        let larger = Look {
            cell_px: (look.cell_px.0 * zoom, look.cell_px.1 * zoom),
            ..*look
        };
        let fewer: Vec<Vec<Tile<'_>>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|t| Tile {
                        cols: smaller(t.cols, zoom),
                        ..*t
                    })
                    .collect()
            })
            .collect();
        art = painter.paint(&fewer, heading, &larger)?;
    }
    Ok(art)
}

/// `cols` over `zoom`, rounded up so the card keeps at least its width.
fn smaller(cols: u16, zoom: f32) -> u16 {
    let fewer = whole(
        f64::from(f32::from(cols) / zoom).ceil(),
        u32::from(u16::MAX),
    );
    u16::try_from(fewer).unwrap_or(cols).max(1)
}
