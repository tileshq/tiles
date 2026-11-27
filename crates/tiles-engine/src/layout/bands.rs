//! Range bands: a spread per category from bounds the intent sends.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f, symbol_radius};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The plot height, as the gallery's range bands set it.
const HEIGHT: f32 = 210.0;

/// Most room one row takes, so a few rows stay close together.
const MAX_STEP: f32 = 48.0;

/// A band's height, as the gallery sets it; its ends are fully round.
const BAND: f32 = 12.0;

/// The outer and inner bands' opacity over the card, as the gallery's light
/// outer band and its 55% inner band read; grey bands beside a named one
/// take more, or a dark card all but hides them.
const OUTER_OPACITY: f32 = 0.3;
const INNER_OPACITY: f32 = 0.7;
const GREY_OPACITY: (f32, f32) = (0.55, 1.0);

/// The middle dot, as a Vega symbol size.
const DOT: f32 = 70.0;

/// Ticks on the value axis, as the gallery's range bands ask for.
const TICKS: f64 = 6.0;

/// One row's bounds.
struct Row {
    label: String,
    index: usize,
    lo: f64,
    hi: f64,
    mid: Option<f64>,
    inner: Option<(f64, f64)>,
}

/// A light band from low to high per category, an optional darker inner
/// band and a dot at the middle value, each on a faint row line over a
/// value axis; the named rows take the accent (red for a bad finding) and
/// the rest are grey.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn bands(c: &mut Ctx) -> f32 {
    let num = |role: &str, r: &[tiles_intent::Cell]| c.col(role).and_then(|i| r[i].number());
    let x = c.col("x").unwrap_or(0);
    let rows: Vec<Row> = c
        .intent
        .table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            Some(Row {
                label: r[x].label(),
                index: i,
                lo: num("lo", r)?,
                hi: num("hi", r)?,
                mid: num("mid", r),
                inner: num("inner_lo", r).zip(num("inner_hi", r)),
            })
        })
        .collect();
    let ls = c.label_style();
    let names: Vec<String> = rows.iter().map(|r| r.label.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    let all: Vec<f64> = rows
        .iter()
        .flat_map(|r| {
            [
                Some(r.lo),
                Some(r.hi),
                r.mid,
                r.inner.map(|i| i.0),
                r.inner.map(|i| i.1),
            ]
            .into_iter()
            .flatten()
        })
        .collect();
    let min = all.iter().copied().fold(f64::INFINITY, f64::min);
    let max = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let from = if min >= 0.0 && min <= max / 2.0 {
        0.0
    } else {
        min
    };
    let (lo, hi) = nice(from, max, TICKS);
    let value_ticks = ticks(lo, hi, TICKS);
    let tf = c.tick_format(&value_ticks, true);
    let nf = NumberFormat::for_values(&all).with_unit(c.intent.unit);
    let left = c.left + PAD + widest + c.t.marks.label_padding + BAND / 2.0;
    let right = (c.left + c.width - PAD - c.width_of(&tf.format(hi), ls) / 2.0).max(left + 1.0);
    let scale = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let step = (c.t.marks.row_step + 4.0).max((HEIGHT / f(rows.len().max(1))).min(MAX_STEP));
    let top = c.top + PAD;
    let span = step * f(rows.len());
    let dot = symbol_radius(DOT);
    for i in 0..rows.len() {
        let cy = top + step * (f(i) + 0.5);
        let grid = Shape::Line {
            points: vec![(left - BAND / 2.0, cy), (right + BAND / 2.0, cy)],
            width: c.t.marks.grid_width,
        };
        c.items
            .push(Item::new(Part::Grid, grid, Emphasis::Grid, c.pal.grid));
    }
    for (i, r) in rows.iter().enumerate() {
        let cy = top + step * (f(i) + 0.5);
        let hl = c.is_highlight(&r.label);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            c.left + PAD + widest,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            (Emphasis::Muted, c.pal.muted),
        );
        let tip = c.tip(&[r.index], &nf);
        let (e, col) = c.mark_color(hl);
        let (outer, inner) = if e == Emphasis::Grey {
            GREY_OPACITY
        } else {
            (OUTER_OPACITY, INNER_OPACITY)
        };
        let band = |a: f64, b: f64| {
            let (x0, x1) = (scale.map(a.min(b)), scale.map(a.max(b)));
            // A band narrower than its round ends still reads as a pill.
            let (x0, x1) = if x1 - x0 < BAND {
                let mid = f32::midpoint(x0, x1);
                (mid - BAND / 2.0, mid + BAND / 2.0)
            } else {
                (x0, x1)
            };
            Shape::Rect {
                rect: Rect::from_ltrb(x0, cy - BAND / 2.0, x1, cy + BAND / 2.0),
                radius: BAND / 2.0,
                corners: Corners::All,
            }
        };
        c.items.push(
            Item::new(Part::Mark, band(r.lo, r.hi), e, col.with_opacity(outer))
                .datum(r.index, tip.clone())
                .highlighted(hl)
                .grow(Grow::Fade),
        );
        if let Some((a, b)) = r.inner {
            c.items.push(
                Item::new(Part::Mark, band(a, b), e, col.with_opacity(inner))
                    .datum(r.index, tip.clone())
                    .highlighted(hl)
                    .grow(Grow::Fade),
            );
        }
        if let Some(m) = r.mid {
            // The middle dot is ink in every row, as the gallery's median
            // is: the band carries the name.
            let ring = Shape::Circle {
                cx: scale.map(m),
                cy,
                r: dot + 1.0,
            };
            c.items
                .push(Item::new(Part::Guide, ring, Emphasis::Card, c.pal.card).grow(Grow::Fade));
            let shape = Shape::Circle {
                cx: scale.map(m),
                cy,
                r: dot,
            };
            c.items.push(
                Item::new(Part::Mark, shape, Emphasis::Ink, c.pal.ink)
                    .datum(r.index, tip)
                    .grow(Grow::Fade),
            );
        }
    }
    let bottom = top + span;
    let tick_labels: Vec<(String, f32)> = value_ticks
        .iter()
        .map(|v| (tf.format(*v), scale.map(*v)))
        .collect();
    let below = c.bottom_labels(&tick_labels, bottom, Under::Values);
    let titled = c.axis_title((left, right), bottom + below);
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    bottom + below + titled + PAD - c.top
}
