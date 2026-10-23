//! The card's text result.

use std::fmt::Write as _;

use super::{Card, Face};
use crate::instructions::Draw;

/// The result's first sentence: what was drawn, its id and its cells.
fn opening(card: &Card) -> String {
    let clean = |s: &str| tiles_term::sanitize::line(s, 200);
    let (cols, rows) = card.cells;
    match card.faces.as_slice() {
        [face] => format!(
            "Tiles drew \"{}\" as a {} card (id {}, {cols}x{rows} cells).",
            clean(&face.title),
            face.form,
            card.id,
        ),
        faces => {
            let list: Vec<String> = faces
                .iter()
                .map(|f| format!("\"{}\" ({})", clean(&f.title), f.form))
                .collect();
            let what = match &card.heading {
                Some(title) => format!(
                    "a dashboard \"{}\" of {} cards in {} rows",
                    clean(title),
                    faces.len(),
                    card.lines
                ),
                None if card.lines == 1 => {
                    format!("{} cards side by side in one row", faces.len())
                }
                None if card.lines == faces.len() => {
                    format!("{} cards one above the other", faces.len())
                }
                None => format!("{} cards in {} rows", faces.len(), card.lines),
            };
            format!(
                "Tiles drew {what} as one picture (id {}, {cols}x{rows} cells): {}.",
                card.id,
                list.join(", ")
            )
        }
    }
}

/// The card's text result: what every host shows when it draws no image,
/// and what the model reads. It never holds the picture. The values on the
/// card are listed for rows the model sent (ADR-005); for rows Tiles
/// fetched from Omni or a warehouse they stay on this machine unless the
/// origin is in `shared` (`TILES_OMNI_VALUES_TO_MODEL=1` shares Omni's,
/// `TILES_WAREHOUSE_VALUES_TO_MODEL=1` the warehouse's), so the model sees
/// only the title it wrote, the form and the id. Under [`Draw::Block`] it
/// asks the model to copy the placeholder block that `tiles run` draws over.
/// Under [`Draw::Text`] with `companion`, where `tiles run`'s companion
/// window already shows the card ([`crate::companion_from_env`], ADR-046),
/// it says so and gives the PNG path for reference only, with no `tiles
/// open` line.
#[must_use]
pub fn text_result(card: &Card, draw: Draw, companion: bool, shared: &[String]) -> String {
    let clean = |s: &str| tiles_term::sanitize::line(s, 200);
    let shared = |f: &Face| f.fetched.as_ref().is_none_or(|o| shared.contains(o));
    let (cols, rows) = card.cells;
    let mut out = opening(card);
    let mut notes: Vec<&str> = card.stacked.iter().map(String::as_str).collect();
    for note in card.faces.iter().flat_map(|f| &f.notes) {
        if !notes.contains(&note.as_str()) {
            notes.push(note);
        }
    }
    for note in notes {
        out.push(' ');
        out.push_str(&clean(note));
    }
    if draw == Draw::Block {
        let lines = tiles_term::block::lines(&card.id, rows, &card.title, usize::from(cols));
        let _ = write!(
            out,
            " Put all {rows} lines of this block in your reply exactly as written and in order, \
             inside the ```tiles code fence shown, fence lines included; the terminal draws the \
             chart in their place, so no link, file or artifact is needed.\n\n```tiles\n{}\n```\n",
            lines.join("\n")
        );
    } else if draw == Draw::Text && companion {
        out.push_str(
            " The user already sees the card in the Tiles window beside this terminal; say so \
             in one line and do not ask them to open anything. The PNG path below is for \
             reference.\n",
        );
    } else if draw == Draw::Text {
        out.push_str(" This terminal shows no pictures, so the card was saved as a PNG; give the user its path.\n");
    } else {
        out.push_str(" The user sees the chart below this result.\n");
    }
    let path = card.path.display().to_string();
    let one = card.faces.len() == 1;
    for face in &card.faces {
        let summary = tiles_term::fallback::summary(&face.title, &face.values, &path);
        if shared(face) {
            if one {
                out.push_str("\nValues on the card:\n");
            } else {
                let _ = writeln!(out, "\nValues on \"{}\":", clean(&face.title));
            }
            for line in &summary[1..summary.len() - 1] {
                out.push_str(line);
                out.push('\n');
            }
        } else if one {
            let _ = writeln!(
                out,
                "\nThe rows came from {}, so the values on the card stay on this machine and \
                 are not in this result.",
                clean(face.fetched.as_deref().unwrap_or_default())
            );
        } else {
            let _ = writeln!(
                out,
                "\nThe rows of \"{}\" came from {}, so its values stay on this machine and are \
                 not in this result.",
                clean(&face.title),
                clean(face.fetched.as_deref().unwrap_or_default())
            );
        }
    }
    if let Some(png) = tiles_term::fallback::summary("", &[], &path).last() {
        out.push_str(png);
        out.push('\n');
    }
    if cfg!(target_os = "macos") && !(draw == Draw::Text && companion) {
        // `tiles open` shows the card in a window of its own (ADR-023).
        let _ = write!(
            out,
            "The user can open this card in a window of its own by running `tiles open {}`.",
            card.id
        );
        if card.faces.len() > 1 || draw == Draw::Text {
            out.push_str(" Tell them so in a few words.");
        }
        out.push('\n');
    }
    if card.faces.iter().all(shared) {
        out.push_str(
            "\nAfter the chart, add one or two lines of your own; do not repeat the numbers.",
        );
    } else {
        out.push_str(
            "\nAfter the chart, add one line of your own; do not state or guess any of its numbers.",
        );
    }
    out
}
