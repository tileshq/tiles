//! The pane's cards and what a click does to them (ADR-063, ADR-065):
//! which column a clicked mark selects, the other cards' rows filtered to
//! it in memory, and which card, mark and legend entry sit under a pixel.
//! Pure logic, no terminal.

use tiles_engine::hit::Hit;
use tiles_engine::pane::LegendHit;
use tiles_engine::{Form, Scene};
use tiles_intent::{Cell, Intent};

/// The roles whose column a click selects by, in the order tried: a
/// series first, since a series is what other cards break down by, then
/// the category or time axis.
const SELECT_ROLES: [&str; 2] = ["series", "x"];

/// What a click selected: a column, the label the clicked row holds in
/// it, and the card clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The card clicked, as its intent's index.
    pub card: usize,
    /// The column name.
    pub column: String,
    /// The label.
    pub value: String,
}

/// One card on the pane: its scene, its hover targets, and where it sits.
#[derive(Debug, Clone)]
pub struct Slot {
    /// The intent it draws, as an index into the call's intents; `None`
    /// for the dashboard's heading and the stress scene.
    pub intent: Option<usize>,
    /// How it lined up with its row, to lay it out again the same way.
    pub align: Option<tiles_engine::Align>,
    /// Its top-left cell: column, row.
    pub at: (u16, u16),
    /// Columns and rows it covers.
    pub cells: (u16, u16),
    /// The scene drawn.
    pub scene: Scene,
    /// Its hover targets, in device pixels from its top left.
    pub hits: Vec<Hit>,
    /// Its legend entries, in device pixels from its top left.
    pub legend: Vec<LegendHit>,
}

/// The column a click on `row` of `intent` selects by: the first of
/// [`SELECT_ROLES`] whose cell is text and whose column another card's
/// table also holds; else the first such text cell its form highlights
/// by, which highlights the card and filters nothing. `None` for a row
/// with no text cell in those roles, such as a scorecard's.
#[must_use]
pub fn choose(
    intent: &Intent,
    row: usize,
    others: &[&Intent],
    card: usize,
    forms: &[Form],
) -> Option<Selection> {
    let cells = intent.table.rows.get(row)?;
    let mut first = None;
    for role in SELECT_ROLES {
        let Some(name) = intent.fields.get(role) else {
            continue;
        };
        let Some(c) = intent.table.index(name) else {
            continue;
        };
        let Some(Cell::Text(value)) = cells.get(c) else {
            continue;
        };
        let pick = Selection {
            card,
            column: name.clone(),
            value: value.clone(),
        };
        if others.iter().any(|o| o.table.index(name).is_some()) {
            return Some(pick);
        }
        if colours(intent, forms, c) {
            first.get_or_insert(pick);
        }
    }
    first
}

/// Whether the form of `intent`, found in `forms`, colours the values in
/// its column `column` when they are highlighted.
fn colours(intent: &Intent, forms: &[Form], column: usize) -> bool {
    forms
        .iter()
        .find(|f| f.name == intent.form)
        .is_some_and(|f| f.highlight_columns(intent).contains(&column))
}

/// What a click on `row` of card `card` selects, read from `shown`, the
/// intents the cards were last drawn from: a hit's row counts into the
/// card's table as drawn, filtered or not.
#[must_use]
pub fn pick(shown: &[Intent], card: usize, row: usize, forms: &[Form]) -> Option<Selection> {
    let others: Vec<&Intent> = shown
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != card)
        .map(|(_, i)| i)
        .collect();
    choose(shown.get(card)?, row, &others, card, forms)
}

/// `intent` with only the rows whose `column` reads `value`, and only the
/// highlights those rows still hold; `None` when it has no such column,
/// so the card stays as it is.
#[must_use]
pub fn filtered(intent: &Intent, column: &str, value: &str) -> Option<Intent> {
    let c = intent.table.index(column)?;
    let mut out = intent.clone();
    out.table
        .rows
        .retain(|r| r.get(c).is_some_and(|cell| cell.label() == value));
    out.highlight.retain(|h| {
        out.table
            .rows
            .iter()
            .any(|r| r.iter().any(|v| v.label() == *h))
    });
    Some(out)
}

/// `intent` with `value` as its only highlight, when its form, found in
/// `forms`, highlights by `column` and its rows hold `value` there. A
/// value its form cannot colour, such as the month column a drilled card
/// of weeks still holds, or a stacked bar's x, highlights nothing: the
/// engine would refuse the card.
#[must_use]
pub fn highlighted(intent: &Intent, forms: &[Form], column: &str, value: &str) -> Intent {
    let mut out = intent.clone();
    let takes = intent.table.index(column).is_some_and(|c| {
        colours(intent, forms, c)
            && intent
                .table
                .rows
                .iter()
                .any(|r| r.get(c).is_some_and(|v| v.label() == value))
    });
    if takes {
        out.highlight = vec![value.to_string()];
    }
    out
}

/// The intents a selection draws, from the call's own `base`: the clicked
/// card highlights the value, every other card with the column keeps its
/// rows with that value, and the rest stay as they are.
#[must_use]
pub fn apply(base: &[Intent], selection: Option<&Selection>, forms: &[Form]) -> Vec<Intent> {
    let Some(s) = selection else {
        return base.to_vec();
    };
    base.iter()
        .enumerate()
        .map(|(i, intent)| {
            if i == s.card {
                highlighted(intent, forms, &s.column, &s.value)
            } else {
                filtered(intent, &s.column, &s.value).unwrap_or_else(|| intent.clone())
            }
        })
        .collect()
}

/// The slot under device pixel `x`, `y` with cells `cell` pixels in size,
/// and the pixel inside it.
#[must_use]
pub fn slot_at(slots: &[Slot], cell: (u16, u16), x: u32, y: u32) -> Option<(usize, (i32, i32))> {
    let (cw, ch) = (u32::from(cell.0), u32::from(cell.1));
    slots.iter().enumerate().find_map(|(i, s)| {
        let left = u32::from(s.at.0) * cw;
        let top = u32::from(s.at.1) * ch;
        let right = left + u32::from(s.cells.0) * cw;
        let bottom = top + u32::from(s.cells.1) * ch;
        ((left..right).contains(&x) && (top..bottom).contains(&y)).then(|| {
            let dx = i32::try_from(x - left).unwrap_or(i32::MAX);
            let dy = i32::try_from(y - top).unwrap_or(i32::MAX);
            (i, (dx, dy))
        })
    })
}

/// The index of the hover target under `x`, `y`: the one
/// [`tiles_engine::hit::hit_at`] picks.
#[must_use]
pub fn hit_index(hits: &[Hit], x: i32, y: i32) -> Option<usize> {
    let found = tiles_engine::hit::hit_at(hits, x, y)?;
    hits.iter().position(|h| std::ptr::eq(h, found))
}

/// The index of the legend entry under `x`, `y`.
#[must_use]
pub fn legend_index(legend: &[LegendHit], x: i32, y: i32) -> Option<usize> {
    legend.iter().position(|l| l.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::{Selection, Slot, apply, choose, filtered, hit_index, legend_index, pick, slot_at};
    use tiles_engine::Form;
    use tiles_engine::hit::Hit;
    use tiles_intent::Intent;

    fn intent(json: &str) -> Intent {
        tiles_intent::parse(json).unwrap()
    }

    fn forms() -> Vec<Form> {
        tiles_engine::forms::load(None).unwrap()
    }

    fn stacked() -> Intent {
        intent(
            r#"{"v":1,"form":"stacked-bar","title":"Germany grows","claim":{"kind":"compare","tone":"good"},
            "fields":{"x":"month","y":"revenue","series":"region"},
            "data":{"rows":[{"month":"Jan","region":"Germany","revenue":1},{"month":"Jan","region":"France","revenue":1},
            {"month":"Feb","region":"Germany","revenue":2},{"month":"Feb","region":"France","revenue":1}]}}"#,
        )
    }

    fn months() -> Intent {
        intent(
            r#"{"v":1,"form":"line","title":"Revenue climbs","claim":{"kind":"trend","tone":"good"},
            "fields":{"x":"month","y":"revenue"},
            "data":{"rows":[{"month":"Jan","revenue":2},{"month":"Feb","revenue":3}]}}"#,
        )
    }

    fn by_region() -> Intent {
        intent(
            r#"{"v":1,"form":"bar","title":"Germany leads","claim":{"kind":"rank","tone":"good"},
            "fields":{"x":"region","y":"revenue"},"highlight":["Germany"],
            "data":{"rows":[{"region":"Germany","revenue":4},{"region":"France","revenue":3}]}}"#,
        )
    }

    fn by_month() -> Intent {
        intent(
            r#"{"v":1,"form":"multi-line","title":"Germany climbs","claim":{"kind":"compare","tone":"good"},
            "fields":{"x":"month","y":"revenue","series":"region"},"highlight":["Germany"],
            "data":{"rows":[{"month":"Jan","region":"Germany","revenue":1},{"month":"Feb","region":"Germany","revenue":2},
            {"month":"Jan","region":"France","revenue":1},{"month":"Feb","region":"France","revenue":1}]}}"#,
        )
    }

    fn quarters() -> Intent {
        intent(
            r#"{"v":1,"form":"big-number","title":"Revenue","claim":{"kind":"trend","tone":"good"},
            "fields":{"x":"quarter","y":"revenue"},
            "data":{"rows":[{"quarter":"Q1","revenue":1},{"quarter":"Q2","revenue":2}]}}"#,
        )
    }

    #[test]
    fn a_click_selects_by_the_column_other_cards_share() {
        let forms = forms();
        let (bars, lines, kpi) = (by_region(), by_month(), quarters());
        let s = choose(&bars, 1, &[&lines, &kpi], 0, &forms).unwrap();
        assert_eq!((s.column.as_str(), s.value.as_str()), ("region", "France"));
        // On the lines the series is shared, not the month.
        let s = choose(&lines, 0, &[&bars, &kpi], 1, &forms).unwrap();
        assert_eq!((s.column.as_str(), s.value.as_str()), ("region", "Germany"));
        // A column nobody shares still selects, to highlight.
        let s = choose(&bars, 1, &[&kpi], 0, &forms).unwrap();
        assert_eq!((s.column.as_str(), s.value.as_str()), ("region", "France"));
        // A scorecard highlights nothing, so its unshared column selects
        // nothing either.
        assert_eq!(choose(&kpi, 1, &[&bars], 2, &forms), None);
        assert_eq!(choose(&kpi, 9, &[&bars], 2, &forms), None);
    }

    #[test]
    fn filtering_keeps_matching_rows_and_the_highlights_they_hold() {
        let lines = filtered(&by_month(), "region", "France").unwrap();
        assert_eq!(lines.table.rows.len(), 2);
        assert!(lines.highlight.is_empty(), "Germany is gone from the rows");
        assert_eq!(filtered(&quarters(), "region", "France"), None);
    }

    #[test]
    fn a_selection_highlights_its_card_and_filters_the_others() {
        let base = vec![by_region(), by_month(), quarters()];
        let s = Selection {
            card: 0,
            column: "region".into(),
            value: "France".into(),
        };
        let forms = forms();
        let out = apply(&base, Some(&s), &forms);
        assert_eq!(out[0].highlight, ["France"]);
        assert_eq!(
            out[0].table.rows.len(),
            2,
            "the clicked card keeps its rows"
        );
        assert_eq!(out[1].table.rows.len(), 2);
        assert_eq!(out[2], base[2]);
        assert_eq!(apply(&base, None, &forms), base);
    }

    #[test]
    fn the_clicked_card_highlights_only_through_the_selected_column() {
        use super::highlighted;
        let forms = forms();
        let bars = by_region();
        assert_eq!(
            highlighted(&bars, &forms, "region", "France").highlight,
            ["France"]
        );
        // The value in a column that fills no category role, or in no row,
        // leaves the highlight as it was.
        assert_eq!(
            highlighted(&bars, &forms, "revenue", "4").highlight,
            ["Germany"]
        );
        assert_eq!(
            highlighted(&bars, &forms, "region", "Spain").highlight,
            ["Germany"]
        );
        let weeks = intent(
            r#"{"v":1,"form":"column","title":"August by week","claim":{"kind":"trend","tone":"good"},
            "fields":{"x":"week","y":"orders"},
            "data":{"rows":[{"month":"2026-08-01","week":"2026-08-03","orders":4},
            {"month":"2026-08-01","week":"2026-08-10","orders":5}]}}"#,
        );
        // A drilled card of weeks still holds the month it was drilled from.
        assert_eq!(
            highlighted(&weeks, &forms, "month", "2026-08-01").highlight,
            Vec::<String>::new()
        );
        assert_eq!(
            highlighted(&weeks, &forms, "week", "2026-08-10").highlight,
            ["2026-08-10"]
        );
    }

    #[test]
    fn a_stacked_card_clicked_by_its_shared_x_draws_unhighlighted() {
        let forms = forms();
        let engine = tiles_engine::Engine::new().unwrap();
        let base = vec![stacked(), months()];
        // Only the line shares a column, the month, so the click selects it.
        let s = pick(&base, 0, 2, &forms).unwrap();
        assert_eq!((s.column.as_str(), s.value.as_str()), ("month", "Feb"));
        let out = apply(&base, Some(&s), &forms);
        // A stacked bar colours only its series, so the month is no highlight.
        assert_eq!(out[0].highlight, Vec::<String>::new());
        let opts = tiles_engine::Options::default();
        assert!(engine.scene(&out[0], &opts).is_ok());
        assert_eq!(out[1].table.rows.len(), 1);
        // Alone, the stack selects and highlights its series.
        let s = pick(&base[..1], 0, 2, &forms).unwrap();
        assert_eq!((s.column.as_str(), s.value.as_str()), ("region", "Germany"));
        assert_eq!(
            apply(&base[..1], Some(&s), &forms)[0].highlight,
            ["Germany"]
        );
    }

    #[test]
    fn a_click_on_a_filtered_card_reads_the_row_it_drew() {
        let base = vec![by_region(), by_month(), quarters()];
        let forms = forms();
        let france = pick(&base, 0, 1, &forms).unwrap();
        assert_eq!(france.value, "France");
        let shown = apply(&base, Some(&france), &forms);
        // The lines now hold France's rows only; their first is France, Jan.
        let s = pick(&shown, 1, 0, &forms).unwrap();
        assert_eq!(
            (s.card, s.column.as_str(), s.value.as_str()),
            (1, "region", "France")
        );
        assert_eq!(pick(&shown, 1, 2, &forms), None, "past the drawn rows");
        assert_eq!(pick(&shown, 9, 0, &forms), None);
    }

    fn slot(at: (u16, u16), cells: (u16, u16)) -> Slot {
        Slot {
            intent: None,
            align: None,
            at,
            cells,
            scene: tiles_engine::Engine::new().unwrap().heading(
                "t",
                None,
                &tiles_engine::Options::default(),
            ),
            hits: Vec::new(),
            legend: Vec::new(),
        }
    }

    #[test]
    fn a_pixel_finds_its_slot_and_the_place_inside_it() {
        let slots = [slot((0, 0), (10, 4)), slot((11, 0), (10, 4))];
        assert_eq!(slot_at(&slots, (8, 16), 5, 5), Some((0, (5, 5))));
        assert_eq!(slot_at(&slots, (8, 16), 88, 63), Some((1, (0, 63))));
        assert_eq!(slot_at(&slots, (8, 16), 80, 5), None, "the gutter");
        assert_eq!(slot_at(&slots, (8, 16), 5, 64), None, "below");
    }

    #[test]
    fn the_smallest_target_under_the_pointer_wins() {
        let hit = |left, top, right, bottom| Hit {
            left,
            top,
            right,
            bottom,
            row: Some(0),
            tip: String::new(),
        };
        let hits = [hit(0, 0, 100, 100), hit(40, 40, 50, 50)];
        assert_eq!(hit_index(&hits, 45, 45), Some(1));
        assert_eq!(hit_index(&hits, 5, 5), Some(0));
        assert_eq!(hit_index(&hits, 200, 5), None);
    }

    #[test]
    fn a_pixel_finds_its_legend_entry() {
        let entry = |left, label: &str| tiles_engine::pane::LegendHit {
            left,
            top: 0,
            right: left + 40,
            bottom: 20,
            label: label.into(),
        };
        let legend = [entry(0, "Espresso"), entry(50, "Filter")];
        assert_eq!(legend_index(&legend, 60, 10), Some(1));
        assert_eq!(legend_index(&legend, 45, 10), None, "between entries");
    }
}
