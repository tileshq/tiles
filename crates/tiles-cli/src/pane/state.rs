//! The pane's interaction state (ADR-065): what the user picked, as one
//! value every card's scene is laid out from. A click on a mark focuses
//! its value on every card (ADR-070), a click on a legend entry hides that series on its card,
//! `s` sorts the card under the pointer, and `d` drills into a period of a
//! card drawn from an Omni date field. Pure logic over intents, no terminal
//! and no network: a drill's rows arrive through [`Drilled`], fetched by
//! the caller through the source seam.

use std::collections::{BTreeMap, BTreeSet};

use tiles_engine::Form;
use tiles_intent::source::{DataSource, OmniSource};
use tiles_intent::{Cell, Intent, Request};

use super::board::{self, Selection};

/// The forms whose bars or dots stand in the order their rows come in, one
/// per category, which the user may change. A bar chart and a lollipop
/// rank their rows themselves, largest first, and a line, area, slope or
/// waterfall keeps its order in time.
const SORTABLE: [&str; 6] = [
    "column",
    "dot-plot",
    "dumbbell",
    "grouped-bar",
    "stacked-bar",
    "stacked-100-bar",
];

/// Omni's date timeframes, coarsest first: a drill goes one step finer.
pub const TIMEFRAMES: [&str; 5] = ["year", "quarter", "month", "week", "date"];

/// A card's sort, which `s` steps through: largest first, smallest first,
/// then the order the card came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Largest first.
    Desc,
    /// Smallest first.
    Asc,
}

impl Order {
    /// The sort after this one; `None` is the card's own order.
    #[must_use]
    pub fn next(current: Option<Self>) -> Option<Self> {
        match current {
            None => Some(Self::Desc),
            Some(Self::Desc) => Some(Self::Asc),
            Some(Self::Asc) => None,
        }
    }

    /// Its name on the status line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Desc => "largest first",
            Self::Asc => "smallest first",
        }
    }
}

/// One drill level of a card: the query asked, the period it narrows to,
/// and the card it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Drilled {
    /// The request with the finer timeframe, which a further drill starts
    /// from.
    pub request: Request,
    /// The coarser column and the period's label in it.
    pub within: (String, String),
    /// The card: the fetched rows inside every period drilled into.
    pub intent: Intent,
}

/// What the user picked on the pane.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The value a click on a mark focused (ADR-070).
    pub focus: Option<Selection>,
    /// Series hidden per card, by its index, through its legend.
    pub hidden: BTreeMap<usize, BTreeSet<String>>,
    /// Each sorted card's order.
    pub sorts: BTreeMap<usize, Order>,
    /// Each drilled card's levels, the deepest last.
    pub drills: BTreeMap<usize, Vec<Drilled>>,
}

impl State {
    /// Whether nothing is picked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.focus.is_none()
            && self.hidden.is_empty()
            && self.sorts.is_empty()
            && self.drills.is_empty()
    }

    /// The intent card `card` draws before any focus, hide or sort: its
    /// deepest drill, else the call's own.
    #[must_use]
    pub fn base<'a>(&'a self, call: &'a [Intent], card: usize) -> Option<&'a Intent> {
        self.drills
            .get(&card)
            .and_then(|levels| levels.last())
            .map(|d| &d.intent)
            .or_else(|| call.get(card))
    }

    /// The intents the cards draw: each card's [`State::base`], its hidden
    /// series left out, the focus applied, then its sort.
    /// `forms` are the engine's, to tell which columns a card highlights by.
    #[must_use]
    pub fn apply(&self, call: &[Intent], forms: &[Form]) -> Vec<Intent> {
        let mut out: Vec<Intent> = (0..call.len())
            .filter_map(|i| self.base(call, i).cloned())
            .collect();
        for (card, names) in &self.hidden {
            if let Some(intent) = out.get_mut(*card) {
                *intent = without(intent, names);
            }
        }
        let mut out = board::apply(&out, self.focus.as_ref(), forms);
        for (card, order) in &self.sorts {
            if let Some(intent) = out.get_mut(*card) {
                *intent = sorted(intent, *order);
            }
        }
        out
    }

    /// Hides series `name` on card `card`, or shows it again when hidden.
    /// A card keeps at least one series: hiding its last is refused.
    ///
    /// # Errors
    ///
    /// The series is the last one the card shows.
    pub fn toggle(&mut self, call: &[Intent], card: usize, name: &str) -> Result<(), String> {
        let names = self.base(call, card).map(series).unwrap_or_default();
        let hidden = self.hidden.entry(card).or_default();
        if !hidden.remove(name) {
            let left = names.iter().filter(|s| !hidden.contains(*s)).count();
            if left <= 1 {
                return Err(format!("{name} is the last series on its card"));
            }
            hidden.insert(name.to_string());
        }
        self.hidden.retain(|_, h| !h.is_empty());
        Ok(())
    }

    /// Whether `picked` is the focused value: the same column and value,
    /// on whichever card it was clicked.
    #[must_use]
    pub fn focuses(&self, picked: &Selection) -> bool {
        self.focus
            .as_ref()
            .is_some_and(|f| f.column == picked.column && f.value == picked.value)
    }

    /// Drops the focus when its card's drill level differs from
    /// `prev`'s: the card now draws another period, which the selection
    /// was not made on.
    pub fn redrilled(&mut self, prev: &Self) {
        if self
            .focus
            .as_ref()
            .is_some_and(|f| self.depth(f.card) != prev.depth(f.card))
        {
            self.focus = None;
        }
    }

    /// How many drill levels card `card` is down.
    fn depth(&self, card: usize) -> usize {
        self.drills.get(&card).map_or(0, Vec::len)
    }

    /// Gives each card in `refused`, the cards that kept their picture,
    /// its drill levels, hidden series and sort from `prev`, so the state
    /// records only what the cards draw.
    pub fn keep(&mut self, prev: &Self, refused: &[usize]) {
        for &card in refused {
            restore(&mut self.drills, &prev.drills, card);
            restore(&mut self.hidden, &prev.hidden, card);
            restore(&mut self.sorts, &prev.sorts, card);
        }
    }

    /// Steps card `card`'s sort, skipping an order its rows, `intent` as
    /// drawn, already stand in; or says the card has no order to change.
    ///
    /// # Errors
    ///
    /// The card's form keeps its own order.
    pub fn sort(&mut self, intent: &Intent, card: usize) -> Result<Option<Order>, String> {
        if !SORTABLE.contains(&intent.form.as_str()) {
            return Err(format!(
                "a {} keeps its own order; s sorts columns, stacks and dots",
                intent.form
            ));
        }
        let mut next = Order::next(self.sorts.get(&card).copied());
        if let Some(o) = next
            && sorted(intent, o).table == intent.table
        {
            next = Order::next(next);
        }
        match next {
            Some(o) => self.sorts.insert(card, o),
            None => self.sorts.remove(&card),
        };
        Ok(next)
    }
}

/// Card `card`'s entry in `into` set back to the one in `from`.
fn restore<V: Clone>(into: &mut BTreeMap<usize, V>, from: &BTreeMap<usize, V>, card: usize) {
    match from.get(&card) {
        Some(v) => into.insert(card, v.clone()),
        None => into.remove(&card),
    };
}

/// The distinct labels of `intent`'s series column, in row order.
#[must_use]
pub fn series(intent: &Intent) -> Vec<String> {
    let Some(c) = intent.column("series") else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    for row in &intent.table.rows {
        let label = row.get(c).map(Cell::label).unwrap_or_default();
        if !seen.contains(&label) {
            seen.push(label);
        }
    }
    seen
}

/// The series a legend entry drawn as `label` names on `intent`: the one
/// with that name, or the one name a label cut with an ellipsis begins.
#[must_use]
pub fn legend_series(intent: &Intent, label: &str) -> Option<String> {
    let names = series(intent);
    if let Some(exact) = names.iter().find(|n| *n == label) {
        return Some(exact.clone());
    }
    let stem = label.strip_suffix('…')?;
    let mut starts = names.iter().filter(|n| n.starts_with(stem));
    match (starts.next(), starts.next()) {
        (Some(one), None) => Some(one.clone()),
        _ => None,
    }
}

/// `intent` without the rows of the series `names`, and without the
/// highlights only those rows held.
#[must_use]
pub fn without(intent: &Intent, names: &BTreeSet<String>) -> Intent {
    let Some(c) = intent.column("series") else {
        return intent.clone();
    };
    let mut out = intent.clone();
    out.table
        .rows
        .retain(|r| r.get(c).is_none_or(|cell| !names.contains(&cell.label())));
    out.highlight.retain(|h| {
        !names.contains(h)
            && out
                .table
                .rows
                .iter()
                .any(|r| r.iter().any(|v| v.label() == *h))
    });
    out
}

/// The measure a card's order goes by: `y`, or a dumbbell's `hi`.
fn measure(intent: &Intent) -> Option<usize> {
    intent.column("y").or_else(|| intent.column("hi"))
}

/// `intent` with its rows in `order` of each category's total: a category
/// with several series moves as one, and categories with equal totals keep
/// the order they came in.
#[must_use]
pub fn sorted(intent: &Intent, order: Order) -> Intent {
    let (Some(x), Some(y)) = (intent.column("x"), measure(intent)) else {
        return intent.clone();
    };
    let mut totals: Vec<(String, f64)> = Vec::new();
    for row in &intent.table.rows {
        let label = row.get(x).map(Cell::label).unwrap_or_default();
        let value = match row.get(y) {
            Some(Cell::Number(n)) => *n,
            _ => 0.0,
        };
        match totals.iter_mut().find(|(l, _)| *l == label) {
            Some((_, t)) => *t += value,
            None => totals.push((label, value)),
        }
    }
    let mut ranked: Vec<usize> = (0..totals.len()).collect();
    ranked.sort_by(|a, b| {
        let (ta, tb) = (totals[*a].1, totals[*b].1);
        match order {
            Order::Desc => tb.total_cmp(&ta),
            Order::Asc => ta.total_cmp(&tb),
        }
    });
    let rank = |label: &str| {
        ranked
            .iter()
            .position(|i| totals[*i].0 == label)
            .unwrap_or(usize::MAX)
    };
    let mut out = intent.clone();
    out.table
        .rows
        .sort_by_cached_key(|r| rank(&r.get(x).map(Cell::label).unwrap_or_default()));
    out
}

/// `field` one timeframe finer: `orders.created_at[month]` becomes
/// `orders.created_at[week]`; `None` for a field with no timeframe or at
/// the finest.
#[must_use]
pub fn finer(field: &str) -> Option<String> {
    let open = field.rfind('[')?;
    let frame = field[open + 1..].strip_suffix(']')?;
    let at = TIMEFRAMES.iter().position(|t| *t == frame)?;
    let next = TIMEFRAMES.get(at + 1)?;
    Some(format!("{}[{next}]", &field[..open]))
}

/// The request that drills `request`'s card into the period `label` of
/// its `x` field: the same Omni query with the next finer timeframe beside
/// the drilled one, sorted by it where the query sorted by the drilled one,
/// at the default row limit, its highlight and note dropped since they name
/// the coarser periods.
///
/// # Errors
///
/// The card's rows do not come from an Omni date field with a finer
/// timeframe, or the query already fetches the most fields Omni takes.
pub fn drill(request: &Request, label: &str) -> Result<Request, String> {
    let (Some(source), Some(x)) = (request.omni(), request.fields.get("x")) else {
        return Err("d drills only a card drawn from an Omni date field".into());
    };
    let Some(next) = finer(x) else {
        return Err(if x.contains('[') {
            "this card is at Omni's finest timeframe".into()
        } else {
            "d drills only a card drawn from an Omni date field".into()
        });
    };
    let mut fields = source.fields.clone();
    if !fields.contains(&next) {
        if fields.len() >= tiles_intent::source::MAX_COLUMNS {
            return Err("this card already fetches the most fields Omni takes".into());
        }
        let at = fields
            .iter()
            .position(|f| f == x)
            .map_or(fields.len(), |i| i + 1);
        fields.insert(at, next.clone());
    }
    let mut sorts = source.sorts.clone();
    for sort in &mut sorts {
        if sort.field == *x && !source.sorts.iter().any(|s| s.field == next) {
            sort.field.clone_from(&next);
        }
    }
    let frame = next.rsplit('[').next().unwrap_or("").trim_end_matches(']');
    let mut out = request.clone();
    out.data = DataSource::Omni(OmniSource {
        topic: source.topic.clone(),
        fields,
        sorts,
        limit: None,
    });
    out.fields.insert("x".into(), next.clone());
    out.highlight.clear();
    out.note = None;
    out.subtitle = Some(tiles_intent::text::sanitize_truncated(
        &format!("{label} by {frame}"),
        tiles_intent::MAX_SUBTITLE_CHARS,
    ));
    Ok(out)
}

/// `intent` keeping only the rows inside every period of `within`, each a
/// column and the label it must hold.
#[must_use]
pub fn inside(intent: &Intent, within: &[(String, String)]) -> Intent {
    let mut out = intent.clone();
    for (column, label) in within {
        if let Some(c) = out.table.index(column) {
            out.table
                .rows
                .retain(|r| r.get(c).is_some_and(|v| v.label() == *label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Order, State, drill, finer, inside, legend_series, series, sorted, without};
    use crate::pane::board::Selection;
    use tiles_intent::Intent;

    fn intent(json: &str) -> Intent {
        tiles_intent::parse(json).unwrap()
    }

    fn grouped() -> Intent {
        intent(
            r#"{"v":1,"form":"grouped-bar","title":"Lisbon leads","claim":{"kind":"compare","tone":"good"},
            "fields":{"x":"city","y":"sales","series":"product"},"highlight":["Espresso"],
            "data":{"rows":[
            {"city":"Porto","product":"Espresso","sales":2},{"city":"Porto","product":"Filter","sales":1},
            {"city":"Lisbon","product":"Espresso","sales":5},{"city":"Lisbon","product":"Filter","sales":4},
            {"city":"Braga","product":"Espresso","sales":3},{"city":"Braga","product":"Filter","sales":1}]}}"#,
        )
    }

    fn line() -> Intent {
        intent(
            r#"{"v":1,"form":"line","title":"Orders climb","claim":{"kind":"trend","tone":"good"},
            "fields":{"x":"month","y":"orders"},
            "data":{"rows":[{"month":"Jan","orders":1},{"month":"Feb","orders":2}]}}"#,
        )
    }

    fn column(intent: &Intent, name: &str) -> Vec<String> {
        let c = intent.table.index(name).unwrap();
        intent.table.rows.iter().map(|r| r[c].label()).collect()
    }

    #[test]
    fn a_sort_orders_categories_by_their_total_and_keeps_series_together() {
        let desc = sorted(&grouped(), Order::Desc);
        assert_eq!(
            column(&desc, "city"),
            ["Lisbon", "Lisbon", "Braga", "Braga", "Porto", "Porto"]
        );
        assert_eq!(column(&desc, "product")[..2], ["Espresso", "Filter"]);
        let asc = sorted(&grouped(), Order::Asc);
        assert_eq!(column(&asc, "city")[0], "Porto");
    }

    #[test]
    fn s_steps_through_largest_smallest_and_the_cards_own_order() {
        let mut state = State::default();
        assert_eq!(state.sort(&grouped(), 0), Ok(Some(Order::Desc)));
        assert_eq!(state.sort(&grouped(), 0), Ok(Some(Order::Asc)));
        assert_eq!(state.sort(&grouped(), 0), Ok(None));
        assert!(state.is_empty());
        // Rows already largest first go straight to smallest first.
        let ranked = sorted(&grouped(), Order::Desc);
        assert_eq!(state.sort(&ranked, 0), Ok(Some(Order::Asc)));
        assert!(
            state
                .sort(&line(), 1)
                .unwrap_err()
                .contains("keeps its own order")
        );
    }

    #[test]
    fn a_legend_entry_hides_its_series_and_never_the_last() {
        let call = [grouped()];
        let mut state = State::default();
        state.toggle(&call, 0, "Espresso").unwrap();
        let shown = state.apply(&call, &tiles_engine::forms::load(None).unwrap());
        assert_eq!(column(&shown[0], "product"), ["Filter", "Filter", "Filter"]);
        assert!(
            shown[0].highlight.is_empty(),
            "the hidden series' highlight goes"
        );
        assert!(
            state.toggle(&call, 0, "Filter").is_err(),
            "the last series stays"
        );
        state.toggle(&call, 0, "Espresso").unwrap();
        assert!(state.is_empty());
        assert_eq!(
            state.apply(&call, &tiles_engine::forms::load(None).unwrap()),
            call
        );
    }

    #[test]
    fn a_cut_legend_label_finds_the_one_series_it_begins() {
        let g = grouped();
        assert_eq!(series(&g), ["Espresso", "Filter"]);
        assert_eq!(legend_series(&g, "Filter").as_deref(), Some("Filter"));
        assert_eq!(legend_series(&g, "Espr…").as_deref(), Some("Espresso"));
        assert_eq!(legend_series(&g, "…"), None, "both begin with nothing");
        assert_eq!(legend_series(&g, "Latte"), None);
    }

    #[test]
    fn the_focused_value_is_focused_on_whichever_card_it_is_clicked() {
        let pick = |card: usize, value: &str| Selection {
            card,
            column: "region".into(),
            value: value.into(),
        };
        let state = State {
            focus: Some(pick(0, "Germany")),
            ..State::default()
        };
        assert!(state.focuses(&pick(0, "Germany")));
        assert!(state.focuses(&pick(1, "Germany")), "another card");
        assert!(!state.focuses(&pick(1, "France")));
        assert!(!state.focuses(&Selection {
            column: "city".into(),
            ..pick(0, "Germany")
        }));
        assert!(!State::default().focuses(&pick(0, "Germany")));
    }

    #[test]
    fn hiding_focusing_and_sorting_compose() {
        let call = [grouped(), grouped()];
        let mut state = State {
            focus: Some(Selection {
                card: 0,
                column: "city".into(),
                value: "Braga".into(),
            }),
            ..State::default()
        };
        state.toggle(&call, 1, "Filter").unwrap();
        state.sort(&call[0], 0).unwrap();
        let shown = state.apply(&call, &tiles_engine::forms::load(None).unwrap());
        assert_eq!(shown[0].highlight, ["Braga"]);
        assert_eq!(column(&shown[0], "city")[0], "Lisbon");
        // The other card keeps every city; only its hidden series goes.
        assert_eq!(
            column(&shown[1], "city").len(),
            column(&call[1], "city").len() / 2
        );
        assert!(column(&shown[1], "city").iter().any(|c| c == "Lisbon"));
        assert!(column(&shown[1], "product").iter().all(|p| p == "Espresso"));
        let _ = without(&grouped(), &BTreeSet::new());
    }

    #[test]
    fn a_refused_card_keeps_its_hidden_series_and_sort_and_the_others_change() {
        let call = [grouped(), grouped()];
        let mut prev = State::default();
        prev.toggle(&call, 0, "Filter").unwrap();
        let mut next = prev.clone();
        next.toggle(&call, 0, "Filter").unwrap();
        next.sort(&call[0], 0).unwrap();
        next.toggle(&call, 1, "Filter").unwrap();
        next.sort(&call[1], 1).unwrap();
        next.keep(&prev, &[0]);
        assert_eq!(next.hidden[&0], BTreeSet::from(["Filter".to_string()]));
        assert!(!next.sorts.contains_key(&0));
        assert_eq!(next.hidden[&1], BTreeSet::from(["Filter".to_string()]));
        assert_eq!(next.sorts.get(&1), Some(&Order::Desc));
    }

    #[test]
    fn a_selection_outlives_a_drill_only_on_another_card() {
        let call = [line(), line()];
        let selected = |card| State {
            focus: Some(Selection {
                card,
                column: "month".into(),
                value: "Jan".into(),
            }),
            ..State::default()
        };
        let level = super::Drilled {
            request: by_month(),
            within: ("orders.created_at[month]".into(), "2026-08-01".into()),
            intent: call[0].clone(),
        };
        for (card, kept) in [(0, false), (1, true)] {
            let prev = selected(card);
            let mut next = prev.clone();
            next.redrilled(&prev);
            assert_eq!(next.focus, prev.focus, "no drill keeps the selection");
            next.drills.insert(0, vec![level.clone()]);
            next.redrilled(&prev);
            assert_eq!(next.focus.is_some(), kept, "drill, selection on {card}");
            let mut back = selected(card);
            back.redrilled(&State {
                drills: next.drills.clone(),
                ..State::default()
            });
            assert_eq!(back.focus.is_some(), kept, "undrill, selection on {card}");
        }
    }

    #[test]
    fn a_timeframe_steps_one_finer() {
        assert_eq!(
            finer("orders.created_at[month]").as_deref(),
            Some("orders.created_at[week]")
        );
        assert_eq!(
            finer("orders.created_at[week]").as_deref(),
            Some("orders.created_at[date]")
        );
        assert_eq!(finer("orders.created_at[date]"), None);
        assert_eq!(finer("orders.region"), None);
        assert_eq!(finer("orders.created_at[fortnight]"), None);
    }

    fn by_month() -> tiles_intent::Request {
        tiles_intent::parse_request(
            r#"{"v":1,"form":"column","title":"August is busiest","claim":{"kind":"rank","tone":"good"},
            "fields":{"x":"orders.created_at[month]","y":"orders.count"},"highlight":["2026-08"],
            "note":"Peak","data":{"omni":{"topic":"orders",
            "fields":["orders.created_at[month]","orders.count"],
            "sorts":[{"field":"orders.created_at[month]","desc":false}],"limit":12}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_drill_asks_omni_for_the_next_timeframe_beside_the_drilled_one() {
        let r = drill(&by_month(), "2026-08").unwrap();
        let source = r.omni().unwrap();
        assert_eq!(
            source.fields,
            [
                "orders.created_at[month]",
                "orders.created_at[week]",
                "orders.count"
            ]
        );
        assert_eq!(source.sorts[0].field, "orders.created_at[week]");
        assert_eq!(source.limit, None);
        assert_eq!(r.fields["x"], "orders.created_at[week]");
        assert!(r.highlight.is_empty() && r.note.is_none());
        assert_eq!(r.subtitle.as_deref(), Some("2026-08 by week"));
        let deeper = drill(&r, "2026-08-03").unwrap();
        assert_eq!(deeper.omni().unwrap().fields.len(), 4);
        assert_eq!(deeper.fields["x"], "orders.created_at[date]");
        assert!(drill(&deeper, "2026-08-03").unwrap_err().contains("finest"));
    }

    #[test]
    fn only_an_omni_date_field_drills() {
        let inline = tiles_intent::parse_request(
            r#"{"v":1,"form":"column","title":"t","claim":{"kind":"rank","tone":"good"},
            "fields":{"x":"m","y":"n"},"data":{"rows":[{"m":"a","n":1}]}}"#,
        )
        .unwrap();
        assert!(drill(&inline, "a").unwrap_err().contains("Omni date field"));
    }

    #[test]
    fn a_drilled_card_keeps_only_the_rows_inside_its_period() {
        let weeks = intent(
            r#"{"v":1,"form":"column","title":"t","claim":{"kind":"rank","tone":"good"},
            "fields":{"x":"week","y":"n"},"data":{"rows":[
            {"month":"2026-07","week":"2026-07-27","n":1},{"month":"2026-08","week":"2026-08-03","n":2},
            {"month":"2026-08","week":"2026-08-10","n":3}]}}"#,
        );
        let kept = inside(&weeks, &[("month".into(), "2026-08".into())]);
        assert_eq!(column(&kept, "week"), ["2026-08-03", "2026-08-10"]);
    }
}
