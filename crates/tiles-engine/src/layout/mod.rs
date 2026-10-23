//! Intent to scene: the card's text, then the form's layout (ADR-002,
//! `docs/playbook.md`). Every position is in CSS pixels; the raster scales.
//!
//! Vega's conventions are kept where the prototype relied on them (5 px view
//! padding, band paddings, nice domains, text baseline offsets) so the
//! engine's card lands close to the approved HTML card.

mod bars;
mod bullet;
mod columns;
mod dumbbell;
mod grouped;
mod kpi;
mod line;
mod scatter;
mod shares;
mod slope;
mod stacked;
mod waterfall;

use skia_safe::Rect;
use tiles_intent::{Intent, IntentError, Tone};

use crate::Options;
use crate::color::Rgba;
use crate::format::NumberFormat;
use crate::forms::{Form, Layout, RoleKind, Sort};
use crate::scale::{Linear, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Scene, Shape};
use crate::text::{Fonts, Style};
use crate::tokens::{Palette, Tokens};

use bars::{BarStyle, bars};
use bullet::bullet;
use columns::columns;
use dumbbell::dumbbell;
use grouped::grouped;
use kpi::kpi;
use line::line;
use scatter::scatter;
use shares::shares;
use slope::slope;
use stacked::stacked;
use waterfall::waterfall;

/// Vega's view padding around the chart.
const PAD: f32 = 5.0;
/// A compact scorecard's sparkline height, the gap above it and the gap
/// under its header, in CSS pixels: with a one-line title and no subtitle,
/// 12 rows of 16 px cells.
const COMPACT_SPARK: f32 = 26.0;
const COMPACT_SPARK_GAP: f32 = 8.0;
const COMPACT_CHART_GAP: f32 = 6.0;
/// Vega's text baseline offsets, as a fraction of the font size; Vega
/// rounds each to whole pixels.
const TOP: f32 = 0.79;
const MIDDLE: f32 = 0.30;
/// Where Vega put x labels beyond the label padding: measured against the
/// prototype's card (tests/reference.rs).
const X_LABEL_NUDGE: f32 = 2.0;
/// The prototype's chart is an inline SVG, so the HTML card keeps a line's
/// descent under it; every card keeps the same room.
const INLINE_GAP: f32 = 4.0;
/// Most month labels a time axis writes.
const MAX_MONTH_TICKS: usize = 4;
/// Most categories a categorical form draws before it asks for fewer.
pub const MAX_CATEGORIES: usize = 60;
/// A note's second line sits this many font sizes under its first.
const NOTE_LEADING: f32 = 1.3;
/// How far a note's knockout reaches past each line's text box.
const KNOCKOUT_PAD: f32 = 2.0;
/// Widest a category or series label is set, as a share of the card's inner
/// width; a longer label is cut short with an ellipsis.
const LABEL_SHARE: f32 = 0.35;

#[allow(clippy::cast_precision_loss)] // Counts drawn on a card are small.
fn f(n: usize) -> f32 {
    n as f32
}

/// A period written `YYYY-MM`, as its year and month.
fn year_month(label: &str) -> Option<(u32, usize)> {
    let (y, m) = label.split_once('-')?;
    let ok = y.len() == 4 && m.len() == 2 && y.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit());
    let (y, m): (u32, usize) = (y.parse().ok()?, m.parse().ok()?);
    (ok && (1..=12).contains(&m)).then_some((y, m))
}

/// Periods written `YYYY-MM`, as a time axis writes them: `Jan 26`. With
/// `thin`, as on a point-based time axis, thinned to a calendar-aligned step
/// of 1, 2, 3, 6 or 12 months so at most [`MAX_MONTH_TICKS`] show, as the
/// gallery's month axes do; otherwise every period keeps its label, one per
/// band. `None` when any label is not such a period.
fn month_ticks(labels: &[(String, f32)], thin: bool) -> Option<Vec<(String, f32)>> {
    const NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parsed: Vec<(u32, usize, f32)> = labels
        .iter()
        .map(|(l, x)| year_month(l).map(|(y, m)| (y, m, *x)))
        .collect::<Option<_>>()?;
    if parsed.len() < 2 {
        return None;
    }
    let step = if thin {
        [1, 2, 3, 6, 12]
            .into_iter()
            .find(|s| parsed.iter().filter(|p| (p.1 - 1) % s == 0).count() <= MAX_MONTH_TICKS)
            .unwrap_or(12)
    } else {
        1
    };
    let mut out: Vec<(String, f32)> = parsed
        .iter()
        .filter(|p| (p.1 - 1) % step == 0)
        .map(|(y, m, x)| (format!("{} {:02}", NAMES[m - 1], y % 100), *x))
        .collect();
    if out.is_empty() {
        let (y, m, x) = parsed[0];
        out.push((format!("{} {:02}", NAMES[m - 1], y % 100), x));
    }
    Some(out)
}

/// Whether `text` holds a letter of a right-to-left script: Hebrew, Arabic,
/// Syriac, Thaana, N'Ko and their presentation forms.
fn right_to_left(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
    })
}

/// Whether the first letter of `text` is of a right-to-left script
/// ([`right_to_left`]).
fn starts_right_to_left(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(|c| right_to_left(c.encode_utf8(&mut [0; 4])))
}

/// The radius of a Vega circle of symbol `size` (its square's area), so a
/// gallery spec's sizes carry over as written.
fn symbol_radius(size: f32) -> f32 {
    size.sqrt() / 2.0
}

/// Lays out a validated intent as a scene.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(crate) fn scene(
    tokens: &Tokens,
    fonts: &Fonts,
    form: &Form,
    intent: &Intent,
    opts: &Options,
    align: crate::Align,
) -> Result<Scene, IntentError> {
    let pal = tokens.palette(opts.theme);
    validate(form, intent, pal)?;
    let t = tokens;
    let card_left = t.card.margin_x;
    let card_width = opts.width - 2.0 * t.card.margin_x;
    let inset = t.card.border + t.card.padding;
    let left = card_left + inset;
    let width = card_width - 2.0 * inset;
    let mut items = Vec::new();

    let title_style = Style {
        tracking_em: t.text.title_tracking_em,
        ..Style::new(t.text.title_size, t.text.title_weight)
    };
    let mut y = t.card.margin_top + inset;
    let title = fonts.wrap_balanced(&intent.title, title_style, width);
    let title_pad = align.title_lines.saturating_sub(title.len());
    for line in title {
        let baseline = y + fonts.baseline_in_line(t.text.title_size, t.text.title_line);
        items.push(text_item(
            Part::Title,
            line,
            left,
            baseline,
            title_style,
            Anchor::Start,
            Emphasis::Ink,
            pal.ink,
        ));
        y += t.text.title_line;
    }
    #[allow(clippy::cast_precision_loss)] // A few lines.
    {
        y += t.text.title_line * title_pad as f32;
    }
    // A card among others has no default subtitle: the shared note is
    // written once, above or beside them, not in every card.
    let subtitle = match &intent.subtitle {
        // A subtitle that only repeats the title says nothing.
        Some(s) if s.trim().eq_ignore_ascii_case(intent.title.trim()) => String::new(),
        Some(s) => s.clone(),
        None if align.compact || align.among => String::new(),
        None => default_subtitle(form, intent),
    };
    let sub_style = Style::new(t.text.subtitle_size, 400);
    let sub_lines = fonts.wrap_balanced(&subtitle, sub_style, width);
    let sub_pad = align.subtitle_lines.saturating_sub(sub_lines.len());
    if !sub_lines.is_empty() || sub_pad > 0 {
        y += t.text.subtitle_gap;
    }
    #[allow(clippy::cast_precision_loss)] // A few lines.
    {
        y += t.text.subtitle_line * sub_pad as f32;
    }
    for line in sub_lines {
        let baseline = y + fonts.baseline_in_line(t.text.subtitle_size, t.text.subtitle_line);
        items.push(text_item(
            Part::Subtitle,
            line,
            left,
            baseline,
            sub_style,
            Anchor::Start,
            Emphasis::Muted,
            pal.muted,
        ));
        y += t.text.subtitle_line;
    }
    let top = y + if align.compact {
        COMPACT_CHART_GAP
    } else {
        t.text.chart_gap
    };

    let mut ctx = Ctx {
        t,
        pal,
        fonts,
        intent,
        form,
        left,
        width,
        top,
        items,
        plot: Rect::from_xywh(left, top, width, 0.0),
        y_axis: false,
        note_unplaced: false,
        compact: align.compact,
    };
    let chart_height = match form.layout {
        Layout::Columns => columns(&mut ctx),
        Layout::Bars => bars(&mut ctx, BarStyle::Bar),
        Layout::Lollipop => bars(&mut ctx, BarStyle::Lollipop),
        Layout::Dots => bars(&mut ctx, BarStyle::Dot),
        Layout::Line => line(&mut ctx),
        Layout::Kpi => kpi(&mut ctx),
        Layout::Grouped => grouped(&mut ctx),
        Layout::Stacked => stacked(&mut ctx),
        Layout::Shares => shares(&mut ctx),
        Layout::Slope => slope(&mut ctx),
        Layout::Dumbbell => dumbbell(&mut ctx),
        Layout::Bullet => bullet(&mut ctx),
        Layout::Waterfall => waterfall(&mut ctx),
        Layout::Scatter => scatter(&mut ctx),
    };
    if ctx.note_unplaced {
        return Err(IntentError::new(
            "note",
            format!(
                "the note does not fit beside its mark on a {} px card",
                opts.width
            ),
            "shorten the note, or render the card wider",
        ));
    }

    let card_top = t.card.margin_top;
    // Whole CSS pixels, so the card's edge lands on a pixel at any scale.
    let mut card_bottom = (top + chart_height + INLINE_GAP + inset).ceil();
    let mut height = card_bottom + t.card.margin_bottom;
    if align.min_height > height {
        // Whole pixels at or under the height asked for, so snapping to
        // cells below lands on it.
        card_bottom = (align.min_height - t.card.margin_bottom).floor();
        height = card_bottom + t.card.margin_bottom;
    }
    if let Some(cell) = opts.cell_height.filter(|c| *c > 0.0) {
        let snapped = (height / cell - 0.01).ceil() * cell;
        card_bottom += snapped - height;
        height = snapped;
    }
    Ok(Scene {
        form: form.name.clone(),
        theme: opts.theme,
        width: opts.width,
        height,
        card: Rect::from_ltrb(card_left, card_top, card_left + card_width, card_bottom),
        boxed: true,
        plot: ctx.plot,
        y_axis: ctx.y_axis,
        items: ctx.items,
    })
}

/// A dashboard heading's title size against a card title's, so the
/// heading reads one step above the cards it names.
const HEADING_SCALE: f32 = 1.2;
/// Space under a dashboard heading's last line, above the first row's
/// own margin.
const HEADING_GAP: f32 = 4.0;

/// Lays out a dashboard's heading: its title, and the shared note under it
/// in the subtitle's style, as text with no card box, its left edge on the
/// cards' left edge.
pub(crate) fn heading(
    tokens: &Tokens,
    fonts: &Fonts,
    title: &str,
    note: Option<&str>,
    opts: &Options,
) -> Scene {
    let t = tokens;
    let pal = t.palette(opts.theme);
    let left = t.card.margin_x + t.card.border;
    let width = opts.width - 2.0 * left;
    let size = t.text.title_size * HEADING_SCALE;
    let line = t.text.title_line * HEADING_SCALE;
    let title_style = Style {
        tracking_em: t.text.title_tracking_em,
        ..Style::new(size, t.text.title_weight)
    };
    let mut items = Vec::new();
    let mut y = t.card.margin_top;
    for text in fonts.wrap_balanced(title, title_style, width) {
        let baseline = y + fonts.baseline_in_line(size, line);
        items.push(text_item(
            Part::Title,
            text,
            left,
            baseline,
            title_style,
            Anchor::Start,
            Emphasis::Ink,
            pal.ink,
        ));
        y += line;
    }
    let note_style = Style::new(t.text.subtitle_size, 400);
    let note_lines = note.map_or_else(Vec::new, |n| fonts.wrap_balanced(n, note_style, width));
    if !note_lines.is_empty() {
        y += t.text.subtitle_gap;
    }
    for text in note_lines {
        let baseline = y + fonts.baseline_in_line(t.text.subtitle_size, t.text.subtitle_line);
        items.push(text_item(
            Part::Subtitle,
            text,
            left,
            baseline,
            note_style,
            Anchor::Start,
            Emphasis::Muted,
            pal.muted,
        ));
        y += t.text.subtitle_line;
    }
    let mut height = (y + HEADING_GAP).ceil();
    if let Some(cell) = opts.cell_height.filter(|c| *c > 0.0) {
        height = (height / cell - 0.01).ceil() * cell;
    }
    Scene {
        form: "heading".into(),
        theme: opts.theme,
        width: opts.width,
        height,
        card: Rect::from_xywh(left, t.card.margin_top, width, y - t.card.margin_top),
        boxed: false,
        plot: Rect::from_xywh(left, y, width, 0.0),
        y_axis: false,
        items,
    }
}

fn default_subtitle(form: &Form, intent: &Intent) -> String {
    let col = |role: &str| intent.fields.get(role).cloned();
    match form.layout {
        Layout::Dumbbell => format!(
            "{} and {} by {}",
            col("lo").unwrap_or_default(),
            col("hi").unwrap_or_default(),
            col("x").unwrap_or_default()
        ),
        Layout::Scatter => format!(
            "{} against {}",
            col("y").unwrap_or_default(),
            col("x").unwrap_or_default()
        ),
        _ => match (col("y"), col("x")) {
            (Some(y), Some(x)) => format!("{y} by {x}"),
            (Some(y), None) => y,
            _ => String::new(),
        },
    }
}

/// Checks the intent's fields against the form's roles, its highlight
/// against the columns the form colours by, and its series against the
/// colours `pal` holds for them.
#[allow(clippy::too_many_lines)] // One rule after another, each with its own one-line refusal.
pub(crate) fn validate(form: &Form, intent: &Intent, pal: &Palette) -> Result<(), IntentError> {
    for key in intent.fields.keys() {
        if !form.roles.iter().any(|r| r.name == *key) {
            let names: Vec<&str> = form.roles.iter().map(|r| r.name.as_str()).collect();
            return Err(IntentError::new(
                format!("fields.{key}"),
                format!("form {} has no role {key}", form.name),
                format!("use the roles {}", names.join(", ")),
            ));
        }
    }
    for role in &form.roles {
        let Some(c) = intent.column(&role.name) else {
            if role.required {
                return Err(IntentError::new(
                    "fields",
                    format!("form {} needs role {} ({})", form.name, role.name, role.doc),
                    format!("add \"{}\": \"<column>\" to fields", role.name),
                ));
            }
            continue;
        };
        if role.kind == RoleKind::Number {
            if let Some(r) = intent
                .table
                .rows
                .iter()
                .position(|row| matches!(row[c], tiles_intent::Cell::Text(_)))
            {
                return Err(IntentError::new(
                    format!("fields.{}", role.name),
                    format!(
                        "column \"{}\" holds text in row {r}",
                        intent.table.columns[c]
                    ),
                    "send each value as a plain number, with no separators, symbols or units, \
                     or name a column of numbers",
                ));
            }
            if intent
                .table
                .rows
                .iter()
                .all(|row| row[c].number().is_none())
            {
                return Err(IntentError::new(
                    format!("fields.{}", role.name),
                    format!("column \"{}\" has no numbers", intent.table.columns[c]),
                    "name a column of numbers",
                ));
            }
        }
    }
    let both = |a: &str, b: &str| match (intent.column(a), intent.column(b)) {
        (Some(a), Some(b)) => intent
            .table
            .rows
            .iter()
            .any(|r| r[a].number().is_some() && r[b].number().is_some()),
        _ => true,
    };
    match form.layout {
        Layout::Scatter if !both("x", "y") => {
            return Err(IntentError::new(
                "data.rows",
                "no row holds both an x and a y number",
                "fill both columns in at least one row",
            ));
        }
        Layout::Dumbbell if !both("lo", "hi") => {
            return Err(IntentError::new(
                "data.rows",
                "no row holds both a lo and a hi number",
                "fill both columns in at least one row",
            ));
        }
        Layout::Slope => slope_pairs(intent)?,
        _ => {}
    }
    if intent.note.is_some() && !takes_note(form.layout) {
        return Err(IntentError::new(
            "note",
            format!("form {} does not draw a note", form.name),
            "drop note, or use column, bar, lollipop, dot-plot, line, area, multi-line or dumbbell",
        ));
    }
    // A big number has one value and no category to colour: a highlight
    // there (often the headline the title names) is ignored, not refused.
    if !intent.highlight.is_empty() && form.takes_highlight() {
        let roles = highlight_roles(form.layout);
        let cols: Vec<usize> = roles
            .iter()
            .filter(|r| {
                form.roles
                    .iter()
                    .any(|role| role.name == **r && role.kind == RoleKind::Category)
            })
            .filter_map(|r| intent.column(r))
            .collect();
        if cols.is_empty() {
            return Err(IntentError::new(
                "highlight",
                format!("form {} has no category to highlight", form.name),
                if roles.is_empty() {
                    "drop highlight"
                } else {
                    "name a label column in fields, or drop highlight"
                },
            ));
        }
        intent.require_category_values(&cols)?;
    }
    if matches!(
        form.layout,
        Layout::Columns
            | Layout::Bars
            | Layout::Lollipop
            | Layout::Dots
            | Layout::Grouped
            | Layout::Stacked
            | Layout::Shares
            | Layout::Bullet
            | Layout::Waterfall
            | Layout::Dumbbell
    ) && let Some(x) = intent.column("x")
    {
        let mut seen: Vec<String> = Vec::new();
        for row in &intent.table.rows {
            let l = row[x].label();
            if !seen.contains(&l) {
                seen.push(l);
            }
        }
        if seen.len() > MAX_CATEGORIES {
            return Err(IntentError::new(
                "data.rows",
                format!(
                    "{} categories is more than form {} can label ({MAX_CATEGORIES})",
                    seen.len(),
                    form.name
                ),
                "aggregate to fewer categories or use a line",
            ));
        }
    }
    if let Some(sc) = intent.column("series") {
        let mut seen: Vec<String> = Vec::new();
        for row in &intent.table.rows {
            let l = row[sc].label();
            if !seen.contains(&l) {
                seen.push(l);
            }
        }
        let named: Vec<bool> = seen.iter().map(|s| intent.highlight.contains(s)).collect();
        let filled = matches!(
            form.layout,
            Layout::Grouped | Layout::Stacked | Layout::Shares
        );
        distinct_series_colours(form, &named, filled, pal, intent.claim.tone)?;
    }
    if matches!(form.layout, Layout::Stacked | Layout::Shares)
        && let Some(y) = intent.column("y")
        && intent
            .table
            .rows
            .iter()
            .any(|r| r[y].number().is_some_and(|v| v < 0.0))
    {
        return Err(IntentError::new(
            "data.rows",
            format!("{} needs values of zero or more", form.name),
            "use grouped-bar for signed values",
        ));
    }
    Ok(())
}

/// Whether a layout draws a note beside a highlighted mark.
fn takes_note(layout: Layout) -> bool {
    matches!(
        layout,
        Layout::Columns
            | Layout::Bars
            | Layout::Lollipop
            | Layout::Dots
            | Layout::Line
            | Layout::Dumbbell
    )
}

/// The columns a form's highlight names values in: its category roles
/// among those it colours.
fn highlight_columns(form: &Form, intent: &Intent) -> Vec<usize> {
    highlight_roles(form.layout)
        .iter()
        .filter(|r| {
            form.roles
                .iter()
                .any(|role| role.name == **r && role.kind == RoleKind::Category)
        })
        .filter_map(|r| intent.column(r))
        .collect()
}

/// Corrects what an intent asks of its form wherever the card it then
/// draws shows no wrong number, noting each change in `intent.notes`
/// (ADR-020), so a model's slip draws a card rather than a refusal: a role
/// the form does not have is left out; text in a number column is the
/// number it spells, or a gap when it holds no digit; a note the form does not draw is left out; a
/// highlight on a form without categories is ignored; a highlight value
/// that names a measure's value highlights that value's row; one that names
/// nothing on the card is left out; and of several series named where one
/// can take the accent, the first keeps it. What still breaks a rule is
/// refused by [`validate`].
pub(crate) fn mend(form: &Form, intent: &mut Intent, pal: &Palette) {
    let extra: Vec<String> = intent
        .fields
        .keys()
        .filter(|k| !form.roles.iter().any(|r| r.name == **k))
        .cloned()
        .collect();
    for k in &extra {
        intent.fields.remove(k);
        intent.notes.push(format!(
            "Form {} has no role {k}, so Tiles left it out.",
            form.name
        ));
    }
    for role in form.roles.iter().filter(|r| r.kind == RoleKind::Number) {
        let Some(c) = intent.column(&role.name) else {
            continue;
        };
        let (mut read, mut gaps) = (0, 0);
        for row in &mut intent.table.rows {
            if let tiles_intent::Cell::Text(t) = &row[c] {
                match row[c].plain_number() {
                    Some(n) => {
                        read += 1;
                        row[c] = tiles_intent::Cell::Number(n);
                    }
                    None if t.chars().any(|ch| ch.is_ascii_digit()) => {}
                    None => {
                        gaps += 1;
                        row[c] = tiles_intent::Cell::Null;
                    }
                }
            }
        }
        if read > 0 {
            intent.notes.push(format!(
                "Tiles read {read} text value{} in column \"{}\" as numbers.",
                if read == 1 { "" } else { "s" },
                intent.table.columns[c]
            ));
        }
        if gaps > 0 {
            intent.notes.push(format!(
                "Column \"{}\" held {gaps} text value{} where numbers go; Tiles drew {} as \
                 gaps.",
                intent.table.columns[c],
                if gaps == 1 { "" } else { "s" },
                if gaps == 1 { "it" } else { "them" }
            ));
        }
    }
    if intent.note.is_some() && !takes_note(form.layout) {
        intent.note = None;
        intent.notes.push(format!(
            "Form {} draws no note, so Tiles left it out.",
            form.name
        ));
    }
    mend_highlight(form, intent, pal);
    if intent.note.is_some() && intent.highlight.is_empty() {
        intent.note = None;
        intent
            .notes
            .push("Tiles left the note out, since no highlighted mark carries it.".into());
    }
}

/// [`mend`] for the highlight: ignored on a form without categories, a
/// measure's value read as its row's category, a value that names nothing
/// left out, and one series kept where only one can take the accent.
fn mend_highlight(form: &Form, intent: &mut Intent, pal: &Palette) {
    let asked = intent.highlight.clone();
    if !intent.highlight.is_empty() && !form.takes_highlight() {
        intent.highlight.clear();
        intent.notes.push(format!(
            "Form {} has no category to colour, so Tiles ignored highlight.",
            form.name
        ));
    }
    if !intent.highlight.is_empty() {
        let cols = highlight_columns(form, intent);
        if cols.is_empty() {
            intent.highlight.clear();
            intent.notes.push(format!(
                "Form {} has no category column to colour, so Tiles ignored highlight.",
                form.name
            ));
        }
        let numbers: Vec<usize> = form
            .roles
            .iter()
            .filter(|r| r.kind == RoleKind::Number)
            .filter_map(|r| intent.column(&r.name))
            .collect();
        let mut kept: Vec<String> = Vec::new();
        for h in std::mem::take(&mut intent.highlight) {
            let rows = &intent.table.rows;
            if rows
                .iter()
                .any(|row| cols.iter().any(|&c| row[c].label() == h))
            {
                kept.push(h);
                continue;
            }
            let value = h.trim().replace(',', "").parse::<f64>().ok();
            let row = value.and_then(|v| {
                rows.iter().find(|row| {
                    numbers
                        .iter()
                        .any(|&n| row[n].number().is_some_and(|x| (x - v).abs() < 1e-9))
                })
            });
            match row.and_then(|row| cols.first().map(|&c| row[c].label())) {
                Some(label) => {
                    intent.notes.push(format!(
                        "Highlight \"{h}\" is a value, not a category, so Tiles highlighted \
                         the row it is in."
                    ));
                    if !kept.contains(&label) {
                        kept.push(label);
                    }
                }
                None => intent.notes.push(format!(
                    "Highlight \"{h}\" names nothing on the card, so Tiles left it out."
                )),
            }
        }
        intent.highlight = kept;
    }
    if let Some(sc) = intent.column("series") {
        let mut series: Vec<String> = Vec::new();
        for row in &intent.table.rows {
            let l = row[sc].label();
            if !series.contains(&l) {
                series.push(l);
            }
        }
        let named: Vec<bool> = series
            .iter()
            .map(|s| intent.highlight.contains(s))
            .collect();
        let filled = matches!(
            form.layout,
            Layout::Grouped | Layout::Stacked | Layout::Shares
        );
        let crowded = distinct_series_colours(form, &named, filled, pal, intent.claim.tone)
            .is_err_and(|e| e.key == "highlight");
        if crowded && let Some(first) = series.iter().find(|s| intent.highlight.contains(s)) {
            let first = first.clone();
            intent
                .highlight
                .retain(|h| *h == first || !series.contains(h));
            let which = if asked.contains(&first) {
                first
            } else {
                "the first series".into()
            };
            intent.notes.push(format!(
                "Only one series can take the accent, so Tiles highlighted {which} alone."
            ));
        }
    }
}

/// The roles whose values a layout colours when highlighted, so a highlight
/// it cannot show is refused (ADR-003).
fn highlight_roles(layout: Layout) -> &'static [&'static str] {
    match layout {
        Layout::Kpi => &[],
        Layout::Stacked | Layout::Shares | Layout::Slope => &["series"],
        Layout::Line | Layout::Grouped => &["series", "x"],
        Layout::Scatter => &["label"],
        Layout::Columns
        | Layout::Bars
        | Layout::Lollipop
        | Layout::Dots
        | Layout::Bullet
        | Layout::Waterfall
        | Layout::Dumbbell => &["x"],
    }
}

/// A slope needs two periods and a series with a value in the first and
/// the last.
fn slope_pairs(intent: &Intent) -> Result<(), IntentError> {
    let (Some(x), Some(y), Some(s)) = (
        intent.column("x"),
        intent.column("y"),
        intent.column("series"),
    ) else {
        return Ok(());
    };
    let rows = &intent.table.rows;
    let mut periods: Vec<String> = Vec::new();
    for row in rows {
        let l = row[x].label();
        if !periods.contains(&l) {
            periods.push(l);
        }
    }
    if periods.len() < 2 {
        return Err(IntentError::new(
            "data.rows",
            "slope needs at least two periods in x",
            "add rows for a second period",
        ));
    }
    let series_at = |period: &str| -> std::collections::HashSet<String> {
        rows.iter()
            .filter(|r| r[x].label() == period && r[y].number().is_some())
            .map(|r| r[s].label())
            .collect()
    };
    if series_at(&periods[0]).is_disjoint(&series_at(&periods[periods.len() - 1])) {
        return Err(IntentError::new(
            "data.rows",
            "no series has a y number in both the first and the last period",
            "give at least one series a value in both periods",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // One flat constructor reads better than a builder here.
fn text_item(
    part: Part,
    text: String,
    x: f32,
    y: f32,
    style: Style,
    anchor: Anchor,
    emphasis: Emphasis,
    color: Rgba,
) -> Item {
    Item::new(
        part,
        Shape::Text {
            text,
            x,
            y,
            style,
            anchor,
        },
        emphasis,
        color,
    )
}

/// The box of text `w` wide set at `x` and baseline `y`.
fn text_rect(w: f32, x: f32, y: f32, style: Style, anchor: Anchor) -> Rect {
    let left = match anchor {
        Anchor::Start => x,
        Anchor::Middle => x - w / 2.0,
        Anchor::End => x - w,
    };
    Rect::from_ltrb(left, y - 0.75 * style.size, left + w, y + 0.2 * style.size)
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// The straight-line distance between two boxes, 0 where they touch.
fn distance(a: &Rect, b: &Rect) -> f32 {
    let dx = (a.left - b.right).max(b.left - a.right).max(0.0);
    let dy = (a.top - b.bottom).max(b.top - a.bottom).max(0.0);
    dx.hypot(dy)
}

/// Whether `inner` lies within `outer`, to half a pixel.
fn within(inner: &Rect, outer: &Rect) -> bool {
    inner.left >= outer.left - 0.5
        && inner.right <= outer.right + 0.5
        && inner.top >= outer.top - 0.5
        && inner.bottom <= outer.bottom + 0.5
}

/// The largest `n` up to `hi` for which `fits` holds, searching on the
/// assumption that it holds for every `n` below some bound.
fn longest(hi: usize, fits: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, hi);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

struct Ctx<'a> {
    t: &'a Tokens,
    pal: &'a Palette,
    fonts: &'a Fonts,
    intent: &'a Intent,
    form: &'a Form,
    left: f32,
    width: f32,
    top: f32,
    items: Vec<Item>,
    plot: Rect,
    y_axis: bool,
    note_unplaced: bool,
    /// A scorecard among others ([`crate::Align::compact`]).
    compact: bool,
}

/// One category with its value and source rows.
struct Point {
    label: String,
    value: f64,
    rows: Vec<usize>,
}

/// One category with each role summed over its rows; a role is `None` when
/// none of the rows hold a number for it.
struct Sums {
    label: String,
    values: Vec<Option<f64>>,
    rows: Vec<usize>,
}

impl Ctx<'_> {
    fn label_style(&self) -> Style {
        Style::new(self.t.text.label_size, self.t.text.label_weight)
    }

    fn value_style(&self) -> Style {
        Style::new(self.t.text.value_size, self.t.text.value_weight).tabular()
    }

    /// A ranked row's value: regular weight, beside its mark rather than on it.
    fn row_value_style(&self) -> Style {
        Style::new(self.t.text.value_size, self.t.text.row_value_weight).tabular()
    }

    fn col(&self, role: &str) -> Option<usize> {
        self.intent.column(role)
    }

    fn any_highlight(&self) -> bool {
        !self.intent.highlight.is_empty()
    }

    fn is_highlight(&self, label: &str) -> bool {
        self.intent.highlight.iter().any(|h| h == label)
    }

    /// The highlighted colour: red for a bad finding, else the accent.
    fn named(&self) -> (Emphasis, Rgba) {
        if self.intent.claim.tone == Tone::Bad {
            (Emphasis::Bad, self.pal.bad)
        } else {
            (Emphasis::Accent, self.pal.accent)
        }
    }

    /// A single-series mark's colour (playbook K1 to K3).
    fn mark_color(&self, highlighted: bool) -> (Emphasis, Rgba) {
        if !self.any_highlight() {
            (Emphasis::Accent, self.pal.accent)
        } else if highlighted {
            self.named()
        } else {
            (Emphasis::Grey, self.pal.grey)
        }
    }

    /// A value label's colour (playbook K4).
    fn value_color(&self, highlighted: bool) -> (Emphasis, Rgba) {
        if self.any_highlight() && highlighted {
            self.named()
        } else {
            (Emphasis::Muted, self.pal.muted)
        }
    }

    /// A ranked row's value colour (playbook K4): the named row's in ink,
    /// as the gallery's ranked forms set it, the rest muted.
    fn row_value_color(&self, highlighted: bool) -> (Emphasis, Rgba) {
        if self.any_highlight() && highlighted {
            (Emphasis::Ink, self.pal.ink)
        } else {
            (Emphasis::Muted, self.pal.muted)
        }
    }

    fn width_of(&self, text: &str, style: Style) -> f32 {
        self.fonts.width(text, style)
    }

    /// The labels as set, each within `share` of the width. A label that
    /// does not fit keeps its longest start with an ellipsis. Where two
    /// different labels would then print alike, each instead shows the word
    /// holding the first character where it differs from the others: its
    /// start, an ellipsis and its end from that word when that fits, else a
    /// window from that word with an ellipsis on each cut side.
    fn fit_all(&self, labels: &[String], style: Style, share: f32) -> Vec<String> {
        let max = self.width * share;
        let chars: Vec<Vec<char>> = labels.iter().map(|l| l.chars().collect()).collect();
        let mut shown: Vec<String> = chars.iter().map(|c| self.fit_end(c, style, max)).collect();
        let mut differ: Vec<Option<usize>> = vec![None; labels.len()];
        loop {
            let mut alike: std::collections::HashMap<&str, Vec<usize>> =
                std::collections::HashMap::new();
            for (i, text) in shown.iter().enumerate() {
                alike.entry(text.as_str()).or_default().push(i);
            }
            let mut moved = Vec::new();
            for i in 0..labels.len() {
                let Some(common) = alike[shown[i].as_str()]
                    .iter()
                    .filter(|&&j| labels[j] != labels[i])
                    .map(|&j| {
                        chars[i]
                            .iter()
                            .zip(&chars[j])
                            .take_while(|(a, b)| a == b)
                            .count()
                    })
                    .max()
                else {
                    continue;
                };
                let d = differ[i].map_or(common, |d| d.min(common));
                if differ[i] != Some(d) {
                    differ[i] = Some(d);
                    moved.push(i);
                }
            }
            if moved.is_empty() {
                return shown;
            }
            for i in moved {
                shown[i] = self.fit_around(&chars[i], differ[i].unwrap_or(0), style, max);
            }
        }
    }

    /// One label within `max`: whole when it fits, else its longest start
    /// with an ellipsis. A right-to-left label reads from the right, so its
    /// cut end is on the left, and the ellipsis goes before it to land there.
    fn fit_end(&self, chars: &[char], style: Style, max: f32) -> String {
        let text: String = chars.iter().collect();
        if self.width_of(&text, style) <= max {
            return text;
        }
        let rtl = starts_right_to_left(&text);
        let cut = |n: usize| {
            let kept = chars[..n].iter().collect::<String>();
            if rtl {
                format!("…{}", kept.trim_end())
            } else {
                format!("{}…", kept.trim_end())
            }
        };
        cut(longest(chars.len(), |n| {
            self.width_of(&cut(n), style) <= max
        }))
    }

    /// One label within `max` that shows the word holding character `d`. A
    /// right-to-left label cut on one side only has its ellipsis at the other
    /// logical end, as in `fit_end`, so it draws on the side of the cut.
    fn fit_around(&self, chars: &[char], d: usize, style: Style, max: f32) -> String {
        let fits = |s: &str| self.width_of(s, style) <= max;
        let text: String = chars.iter().collect();
        if fits(&text) || chars.is_empty() {
            return text;
        }
        let rtl = starts_right_to_left(&text);
        let d = d.min(chars.len() - 1);
        let word = chars[..d]
            .iter()
            .rposition(|ch| ch.is_whitespace())
            .map_or(0, |w| w + 1);
        let tail: String = chars[word..].iter().collect();
        let middle = |n: usize| {
            let start = chars[..n].iter().collect::<String>();
            let start = start.trim_end();
            if rtl && start.is_empty() {
                format!("{tail}…")
            } else {
                format!("{start}…{tail}")
            }
        };
        if fits(&middle(0)) {
            return middle(longest(word, |n| fits(&middle(n))));
        }
        let window = |start: usize, n: usize| {
            let (mut before, mut after) = (start > 0, start + n < chars.len());
            if rtl && before != after {
                (before, after) = (after, before);
            }
            let mut s = String::new();
            if before {
                s.push('…');
            }
            s.push_str(
                chars[start..start + n]
                    .iter()
                    .collect::<String>()
                    .trim_end(),
            );
            if after {
                s.push('…');
            }
            s
        };
        let start = if fits(&window(word, d + 1 - word)) {
            word
        } else {
            d
        };
        let n = longest(chars.len() - start, |n| n == 0 || fits(&window(start, n)));
        window(start, n.max(1))
    }

    /// The hover text for a row: each role's column and value.
    fn tip(&self, rows: &[usize], nf: &NumberFormat) -> String {
        self.tip_with(rows, |_| *nf)
    }

    /// The hover text for a row, each number written in its role's format.
    fn tip_with(&self, rows: &[usize], nf: impl Fn(&str) -> NumberFormat) -> String {
        let mut lines = Vec::new();
        for role in &self.form.roles {
            let Some(c) = self.col(&role.name) else {
                continue;
            };
            let name = &self.intent.table.columns[c];
            let value = match role.kind {
                RoleKind::Number => {
                    let sum: f64 = rows
                        .iter()
                        .filter_map(|&r| self.intent.table.rows[r][c].number())
                        .sum();
                    nf(&role.name).format(sum)
                }
                RoleKind::Category => rows
                    .first()
                    .map(|&r| self.intent.table.rows[r][c].label())
                    .unwrap_or_default(),
            };
            lines.push(format!("{name}: {value}"));
        }
        lines.join("\n")
    }

    #[allow(clippy::too_many_arguments)] // Mirrors text_item; a builder would hide the geometry.
    fn text(
        &mut self,
        part: Part,
        text: String,
        x: f32,
        y: f32,
        style: Style,
        anchor: Anchor,
        (e, c): (Emphasis, Rgba),
    ) {
        self.items
            .push(text_item(part, text, x, y, style, anchor, e, c));
    }

    /// The box a text item covers: its set width, from about its cap height
    /// above the baseline to its descent below.
    fn text_box(&self, item: &Item) -> Option<Rect> {
        let Shape::Text {
            text,
            x,
            y,
            style,
            anchor,
        } = &item.shape
        else {
            return None;
        };
        Some(text_rect(
            self.width_of(text, *style),
            *x,
            *y,
            *style,
            *anchor,
        ))
    }

    /// Where a note `w` wide can hang beside the mark `at`: left of it, right
    /// of it, then over it within the plot's `span`, then within the card.
    /// Each spot is its anchor x and anchor.
    fn note_spots(&self, w: f32, at: Rect, gap: f32, span: (f32, f32)) -> [(f32, Anchor); 4] {
        let over = |lo: f32, hi: f32| (at.center_x() - w / 2.0).min(hi - w).max(lo);
        [
            (at.left - gap, Anchor::End),
            (at.right + gap, Anchor::Start),
            (over(span.0, span.1), Anchor::Start),
            (over(self.left, self.left + self.width), Anchor::Start),
        ]
    }

    /// Sets a note as a callout, in muted label type, as `place_note` does,
    /// as far from every mark as a row's value sits from its own, and refuses
    /// it when it fits nowhere. `own` is the noted mark with the value and
    /// name set beside it; the callout must be nearer to it than to any
    /// other text, and within three of its lines of it, so it reads as that
    /// mark's and not as a row label, a tick or a neighbour's value. It
    /// stays right of `from`, the edge of the row-label or tick column.
    fn note(
        &mut self,
        note: &str,
        spots: impl Fn(&Self, f32) -> Vec<(f32, Anchor, f32)>,
        rows: (f32, f32),
        marks: &[Rect],
        (own, from): (Rect, f32),
    ) {
        let look = (self.label_style(), (Emphasis::Muted, self.pal.muted));
        let pad = self.t.marks.row_value_offset;
        let marks: Vec<Rect> = marks.iter().map(|m| m.with_outset((pad, pad))).collect();
        if !self.place_note(note, look, spots, rows, &marks, Some((own, from))) {
            self.note_unplaced = true;
        }
    }

    /// Sets a note in a value's place and type, in ink, as `place_note` does
    /// from that one spot. Returns whether it fit there.
    fn stand_in(
        &mut self,
        note: &str,
        at: (f32, Anchor, f32),
        rows: (f32, f32),
        marks: &[Rect],
    ) -> bool {
        let look = (self.value_style(), (Emphasis::Ink, self.pal.ink));
        self.place_note(note, look, |_, _| vec![at], rows, marks, None)
    }

    /// Sets a note in `look` at the first spot (anchor x, anchor, baseline)
    /// that `spots` gives for its width, moved up and then down between the
    /// `rows` baselines, so it stays on the card and clear of `marks` and of
    /// every text already set (playbook M9). A note given its mark's box and
    /// the label column's edge in `near` also stays right of that edge,
    /// closer to the box than to other text, and a row value's distance
    /// from every text, as `note` says. A note that fits nowhere on one
    /// line tries two. Each line sits on a knockout in the card's colour,
    /// painted over the gridlines, guides and fills set so far. Returns
    /// whether it found a place; the layout around it never moves for it.
    fn place_note(
        &mut self,
        note: &str,
        look: (Style, (Emphasis, Rgba)),
        spots: impl Fn(&Self, f32) -> Vec<(f32, Anchor, f32)>,
        rows: (f32, f32),
        marks: &[Rect],
        near: Option<(Rect, f32)>,
    ) -> bool {
        let (style, color) = look;
        let line_h = (style.size * NOTE_LEADING).round();
        let texts: Vec<Rect> = self.items.iter().filter_map(|i| self.text_box(i)).collect();
        let pad = self.t.marks.row_value_offset;
        for (lines, w) in self.note_shapes(note, style) {
            let widths: Vec<f32> = lines.iter().map(|l| self.width_of(l, style)).collect();
            let (high, low) = (rows.0, rows.1 - line_h * f(lines.len() - 1));
            let fits = |x: f32, b: f32, anchor: Anchor| {
                let boxes: Vec<Rect> = widths
                    .iter()
                    .enumerate()
                    .map(|(i, lw)| text_rect(*lw, x, b + line_h * f(i), style, anchor))
                    .collect();
                let from = near.map_or(self.left, |(_, from)| from);
                // The knockout reaches past the text, and must cover no
                // other text either.
                let clear = boxes.iter().all(|r| {
                    let patch = r.with_outset((KNOCKOUT_PAD, KNOCKOUT_PAD));
                    r.left >= from
                        && r.right <= self.left + self.width
                        && !marks.iter().any(|t| overlaps(t, r))
                        && !texts.iter().any(|t| overlaps(t, &patch))
                });
                clear
                    && near.is_none_or(|(own, _)| {
                        let mut all = boxes[0];
                        for r in &boxes {
                            all.join(r);
                        }
                        let d = distance(&all, &own);
                        d <= 3.0 * line_h
                            && texts.iter().all(|t| {
                                let dt = distance(&all, t);
                                dt >= pad && (dt > d || within(t, &own))
                            })
                    })
            };
            let found = spots(self, w).into_iter().find_map(|(x, anchor, b)| {
                let b = b.min(low).max(high);
                let up = (0..)
                    .map(|k: u16| b - f32::from(k))
                    .take_while(|y| *y >= high);
                let down = (1..)
                    .map(|k: u16| b + f32::from(k))
                    .take_while(|y| *y <= low);
                up.chain(down)
                    .find(|&y| fits(x, y, anchor))
                    .map(|y| (x, anchor, y))
            });
            if let Some((x, anchor, baseline)) = found {
                // Every line's knockout goes down before any line's text, so
                // the second line's patch never covers the first's descenders.
                for (i, lw) in widths.iter().enumerate() {
                    let y = baseline + line_h * f(i);
                    let rect = text_rect(*lw, x, y, style, anchor)
                        .with_outset((KNOCKOUT_PAD, KNOCKOUT_PAD));
                    let shape = Shape::Rect {
                        rect,
                        radius: 0.0,
                        corners: Corners::All,
                    };
                    self.items.push(
                        Item::new(Part::Knockout, shape, Emphasis::Card, self.pal.card)
                            .grow(Grow::Fade),
                    );
                }
                for (i, line) in lines.into_iter().enumerate() {
                    let at = self.items.len();
                    let y = baseline + line_h * f(i);
                    self.text(Part::Value, line, x, y, style, anchor, color);
                    self.items[at].grow = Grow::Fade;
                }
                return true;
            }
        }
        false
    }

    /// The shapes a note may take, one line and then two, each with its
    /// widest line's width.
    fn note_shapes(&self, note: &str, style: Style) -> Vec<(Vec<String>, f32)> {
        std::iter::once(vec![note.to_string()])
            .chain(self.two_lines(note, style))
            .map(|lines| {
                let w = lines
                    .iter()
                    .map(|l| self.width_of(l, style))
                    .fold(0.0, f32::max);
                (lines, w)
            })
            .collect()
    }

    /// The boxes of the marks and guide lines (a dot plot's tracks) drawn so
    /// far, for a note to keep clear of.
    fn mark_boxes(&self) -> Vec<Rect> {
        self.items
            .iter()
            .filter(|i| matches!(i.part, Part::Mark | Part::Guide))
            .map(Item::bounds)
            .collect()
    }

    /// `text` split at the space that leaves its two lines most even.
    fn two_lines(&self, text: &str, style: Style) -> Option<Vec<String>> {
        text.match_indices(' ')
            .map(|(i, _)| (text[..i].trim_end(), text[i..].trim_start()))
            .filter(|(a, b)| !a.is_empty() && !b.is_empty())
            .min_by(|x, y| {
                let wide =
                    |(a, b): &(&str, &str)| self.width_of(a, style).max(self.width_of(b, style));
                wide(x).total_cmp(&wide(y))
            })
            .map(|(a, b)| vec![a.to_string(), b.to_string()])
    }

    /// Categories of role `x` with role `y` summed per category, in data
    /// order or sorted as the form says.
    fn categories(&self, value_role: &str) -> Vec<Point> {
        let mut out: Vec<Point> = self
            .sums(&[value_role], 1)
            .into_iter()
            .map(|s| Point {
                label: s.label,
                value: s.values[0].unwrap_or(0.0),
                rows: s.rows,
            })
            .collect();
        if self.form.options.sort == Sort::ValueDesc {
            out.sort_by(|a, b| b.value.total_cmp(&a.value));
        }
        out
    }

    /// Categories of role `x` in data order with each of `roles` summed per
    /// category. A row counts only when it holds a number for each of the
    /// first `need` roles.
    fn sums(&self, roles: &[&str], need: usize) -> Vec<Sums> {
        let x = self.col("x").unwrap_or(0);
        let cols: Vec<usize> = roles.iter().map(|r| self.col(r).unwrap_or(0)).collect();
        let mut out: Vec<Sums> = Vec::new();
        for (r, row) in self.intent.table.rows.iter().enumerate() {
            if cols[..need].iter().any(|&c| row[c].number().is_none()) {
                continue;
            }
            let label = row[x].label();
            let i = if let Some(i) = out.iter().position(|s| s.label == label) {
                i
            } else {
                out.push(Sums {
                    label,
                    values: vec![None; cols.len()],
                    rows: Vec::new(),
                });
                out.len() - 1
            };
            let sums = &mut out[i];
            for (k, &c) in cols.iter().enumerate() {
                if let Some(v) = row[c].number() {
                    *sums.values[k].get_or_insert(0.0) += v;
                }
            }
            sums.rows.push(r);
        }
        out
    }

    /// Distinct labels of a role's column in order of first appearance.
    fn distinct(&self, role: &str) -> Vec<String> {
        let Some(c) = self.col(role) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for row in &self.intent.table.rows {
            let l = row[c].label();
            if !out.contains(&l) {
                out.push(l);
            }
        }
        out
    }

    /// Labels under the plot, thinned so none overlap, for the axis `under`
    /// names. Returns the height they take below `plot_bottom`.
    fn bottom_labels(&mut self, labels: &[(String, f32)], plot_bottom: f32, under: Under) -> f32 {
        let style = self.label_style();
        let months = month_ticks(labels, under == Under::Points);
        let labels = months.as_deref().unwrap_or(labels);
        let names: Vec<String> = labels.iter().map(|(l, _)| l.clone()).collect();
        let labels: Vec<(String, f32)> = self
            .fit_all(&names, style, LABEL_SHARE)
            .into_iter()
            .zip(labels.iter().map(|(_, x)| *x))
            .collect();
        // A value axis keeps every n-th tick, n set by the widest, so its
        // step stays even.
        let every = if under == Under::Values && labels.len() > 1 {
            let widest = labels
                .iter()
                .map(|(l, _)| self.width_of(l, style))
                .fold(0.0, f32::max);
            (widest + 8.0) / (labels[1].1 - labels[0].1).abs().max(1.0)
        } else {
            1.0
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A small positive count.
        let every = every.ceil().max(1.0) as usize;
        let baseline =
            plot_bottom + self.t.marks.label_padding + X_LABEL_NUDGE + (TOP * style.size).round();
        // Each label is drawn when it clears the one drawn before it by
        // 8 px, so a long name hides only its own neighbours. A label wider
        // than the band beside the card's edge moves in to stay on the card
        // first.
        let mut drawn_to = f32::NEG_INFINITY;
        for (label, x) in labels.iter().step_by(every) {
            let half = self.width_of(label, style) / 2.0;
            let x = x.min(self.left + self.width - half).max(self.left + half);
            if x - half < drawn_to + 8.0 {
                continue;
            }
            drawn_to = x + half;
            let muted = (Emphasis::Muted, self.pal.muted);
            self.text(
                Part::AxisLabel,
                label.clone(),
                x,
                baseline,
                style,
                Anchor::Middle,
                muted,
            );
        }
        self.t.marks.label_padding + style.size
    }

    /// The tick format for an axis: the measure's unit, and whole
    /// thousands written compact.
    fn tick_format(&self, values: &[f64], unit: bool) -> NumberFormat {
        NumberFormat::for_ticks(values).with_unit(if unit { self.intent.unit } else { None })
    }

    /// Horizontal gridlines and tick labels on the left, about one tick per
    /// `tick_spacing`, or `count` ticks when given.
    fn y_axis(&mut self, scale: &Linear, count: Option<f32>, left: f32, right: f32) {
        let style = self.label_style();
        let count = count
            .unwrap_or_else(|| ((scale.r0 - scale.r1).abs() / self.t.marks.tick_spacing).ceil());
        let values = ticks(
            scale.d0.min(scale.d1),
            scale.d0.max(scale.d1),
            f64::from(count),
        );
        let nf = self.tick_format(&values, true);
        for v in values {
            let y = scale.map(v);
            let grid = Shape::Line {
                points: vec![(left, y), (right, y)],
                width: self.t.marks.grid_width,
            };
            self.items
                .push(Item::new(Part::Grid, grid, Emphasis::Grid, self.pal.grid));
            let muted = (Emphasis::Muted, self.pal.muted);
            let x = left - self.t.marks.label_padding;
            self.text(
                Part::AxisLabel,
                nf.format(v),
                x,
                y + (MIDDLE * style.size).round(),
                style,
                Anchor::End,
                muted,
            );
        }
        self.y_axis = true;
    }

    fn tick_label_width(&self, lo: f64, hi: f64, count: f32) -> f32 {
        let style = self.label_style();
        let values = ticks(lo, hi, f64::from(count));
        let nf = self.tick_format(&values, true);
        values
            .iter()
            .map(|v| self.width_of(&nf.format(*v), style))
            .fold(0.0, f32::max)
    }

    /// Legend rows from `y` (their top), wrapped to the card's width.
    /// Returns their height.
    fn legend(&mut self, entries: &[(String, (Emphasis, Rgba))], y: f32) -> f32 {
        self.legend_with(entries, y, false)
    }

    /// Legend rows as [`Ctx::legend`] sets them, with square swatches when
    /// `square`, as the gallery's share bars key their segments.
    fn legend_with(&mut self, entries: &[(String, (Emphasis, Rgba))], y: f32, square: bool) -> f32 {
        let style = Style::new(self.t.text.legend_size, 400);
        let r = self.t.marks.legend_swatch / 2.0;
        let (start, end) = (self.left + PAD, self.left + self.width - PAD);
        let row = self.t.marks.legend_height;
        let mut x = start;
        let mut mid = y + row / 2.0 - 4.0;
        let mut height = row;
        let names: Vec<String> = entries.iter().map(|(l, _)| l.clone()).collect();
        let names = self.fit_all(&names, style, LABEL_SHARE);
        for (label, (_, (e, c))) in names.into_iter().zip(entries) {
            let text_w = self.width_of(&label, style);
            if x > start && x + 2.0 * r + 5.0 + text_w > end {
                x = start;
                mid += row;
                height += row;
            }
            let swatch = if square {
                Shape::Rect {
                    rect: Rect::from_xywh(x, mid - r, 2.0 * r, 2.0 * r),
                    radius: 0.0,
                    corners: Corners::All,
                }
            } else {
                Shape::Circle {
                    cx: x + r,
                    cy: mid,
                    r,
                }
            };
            self.items.push(Item::new(Part::Legend, swatch, *e, *c));
            x += 2.0 * r + 5.0;
            let muted = (Emphasis::Muted, self.pal.muted);
            self.text(
                Part::Legend,
                label,
                x,
                mid + (MIDDLE * style.size).round(),
                style,
                Anchor::Start,
                muted,
            );
            x += text_w + self.t.marks.legend_gap;
        }
        height
    }
}

/// The axis a row of labels under the plot names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Under {
    /// One category per band.
    Bands,
    /// Points in time, whose months thin to a calendar step.
    Points,
    /// Ticks on a value axis.
    Values,
}

/// How one series is coloured.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SeriesColor {
    /// Named by the highlight: the accent, or red for a bad tone.
    Named,
    /// Beside a named line: the k-th context grey, the last for every line
    /// past the list (K1, ADR-047).
    Rest(usize),
    /// Beside a named stack or group: the k-th neutral.
    Neutral(usize),
    /// No series named: the i-th categorical colour (K6).
    Series(usize),
}

impl SeriesColor {
    /// The colour drawn. An index past the series limit wraps; validation
    /// refuses the colour it then shares.
    fn resolve(self, pal: &Palette, tone: Tone) -> (Emphasis, Rgba) {
        match self {
            SeriesColor::Named if tone == Tone::Bad => (Emphasis::Bad, pal.bad),
            SeriesColor::Named => (Emphasis::Accent, pal.accent),
            SeriesColor::Rest(k) => {
                let k = k.min(pal.context.len() - 1);
                (Emphasis::Context(k), pal.context[k])
            }
            SeriesColor::Neutral(k) => {
                let k = k % pal.neutrals.len();
                (Emphasis::Neutral(k), pal.neutrals[k])
            }
            SeriesColor::Series(i) => {
                let i = i % pal.series;
                (Emphasis::Series(i), pal.categorical[i])
            }
        }
    }
}

/// Each series' colour, given which the highlight names. A named series
/// takes the highlight and the rest take ordered greys: lines the context
/// greys, each at least 3:1 on the card, and `filled` series, which sit side
/// by side as areas (stacks, groups), the neutrals. Either way they stay
/// separable without competing with the accent. With no series named, the
/// categorical order.
fn series_scheme(named: &[bool], filled: bool) -> Vec<SeriesColor> {
    let any = named.iter().any(|b| *b);
    (0..named.len())
        .map(|i| {
            let unnamed = named[..i].iter().filter(|n| !**n).count();
            if !any {
                SeriesColor::Series(i)
            } else if named[i] {
                SeriesColor::Named
            } else if filled {
                SeriesColor::Neutral(unnamed)
            } else {
                SeriesColor::Rest(unnamed)
            }
        })
        .collect()
}

/// Refuses a series colouring in which two series draw alike, unless both
/// are lines beside a named line, which share the last context grey past
/// the list (ADR-003: never a silently wrong chart).
fn distinct_series_colours(
    form: &Form,
    named: &[bool],
    filled: bool,
    pal: &Palette,
    tone: Tone,
) -> Result<(), IntentError> {
    let scheme = series_scheme(named, filled);
    let drawn: Vec<Rgba> = scheme.iter().map(|s| s.resolve(pal, tone).1).collect();
    let rest: Vec<usize> = (0..scheme.len())
        .filter(|&i| matches!(scheme[i], SeriesColor::Rest(_)))
        .collect();
    let mut kept: Vec<usize> = Vec::new();
    for (j, colour) in scheme.iter().enumerate() {
        if matches!(colour, SeriesColor::Rest(_)) {
            continue;
        }
        let shared = kept.iter().chain(&rest).any(|&i| drawn[i] == drawn[j]);
        kept.push(j);
        if !shared {
            continue;
        }
        let count = |f: fn(&SeriesColor) -> bool| scheme.iter().filter(|s| f(s)).count();
        return Err(match colour {
            SeriesColor::Named => IntentError::new(
                "highlight",
                format!(
                    "{} series named on {}, only 1 can take the accent",
                    count(|s| *s == SeriesColor::Named),
                    form.name
                ),
                "name one, or drop highlight to give each series its own colour",
            ),
            SeriesColor::Neutral(_) => IntentError::new(
                "fields.series",
                format!(
                    "{} series outside the highlight, at most {} can be told apart",
                    count(|s| matches!(s, SeriesColor::Neutral(_))),
                    pal.neutrals.len()
                ),
                format!(
                    "keep the top {} beside the highlight and group the rest as Other, or pick \
                     another form",
                    pal.neutrals.len() - 1
                ),
            ),
            SeriesColor::Series(_) | SeriesColor::Rest(_) => IntentError::new(
                "fields.series",
                format!(
                    "{} distinct values, at most {} can be told apart",
                    scheme.len(),
                    pal.series
                ),
                format!(
                    "keep the top {} and group the rest as Other, or pick another form",
                    pal.series - 1
                ),
            ),
        });
    }
    Ok(())
}

/// The series of a two-role chart, their colours from [`series_scheme`]
/// and which the highlight names.
fn series_colors(c: &Ctx, series: &[String], filled: bool) -> (Vec<(Emphasis, Rgba)>, Vec<bool>) {
    let hl: Vec<bool> = series.iter().map(|s| c.is_highlight(s)).collect();
    let colors = series_scheme(&hl, filled)
        .into_iter()
        .map(|colour| colour.resolve(c.pal, c.intent.claim.tone))
        .collect();
    (colors, hl)
}

/// A value sum and the rows it came from, per (x, series) or (series, x).
type Cells = Vec<Vec<Option<(f64, Vec<usize>)>>>;

/// Value sums per (x, series).
fn cells(c: &Ctx, xs: &[String], series: &[String]) -> Cells {
    let (xc, yc, sc) = (
        c.col("x").unwrap_or(0),
        c.col("y").unwrap_or(0),
        c.col("series").unwrap_or(0),
    );
    let mut out = vec![vec![None; series.len()]; xs.len()];
    for (r, row) in c.intent.table.rows.iter().enumerate() {
        let Some(v) = row[yc].number() else { continue };
        let xi = xs.iter().position(|l| *l == row[xc].label()).unwrap_or(0);
        let si = series
            .iter()
            .position(|l| *l == row[sc].label())
            .unwrap_or(0);
        let cell: &mut (f64, Vec<usize>) = out[xi][si].get_or_insert((0.0, Vec::new()));
        cell.0 += v;
        cell.1.push(r);
    }
    out
}

/// Pushes label centres apart so none sit closer than `gap`, moving the
/// stack up where it would run below `bottom` but never above `top`.
fn spread(ys: &mut [(usize, f32)], gap: f32, (top, bottom): (f32, f32)) {
    ys.sort_by(|a, b| a.1.total_cmp(&b.1));
    for i in 1..ys.len() {
        if ys[i].1 - ys[i - 1].1 < gap {
            ys[i].1 = ys[i - 1].1 + gap;
        }
    }
    let Some(last) = ys.last_mut() else { return };
    last.1 = last.1.min(bottom);
    for i in (1..ys.len()).rev() {
        if ys[i].1 - ys[i - 1].1 < gap {
            ys[i - 1].1 = ys[i].1 - gap;
        }
    }
    ys[0].1 = ys[0].1.max(top);
    for i in 1..ys.len() {
        if ys[i].1 - ys[i - 1].1 < gap {
            ys[i].1 = ys[i - 1].1 + gap;
        }
    }
}
