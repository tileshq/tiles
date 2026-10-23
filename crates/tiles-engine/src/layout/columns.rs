//! The column chart: vertical bars from zero, values above.

use skia_safe::Rect;

use super::{Ctx, MIDDLE, PAD, Under, text_rect};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice};
use crate::scene::{Anchor, Corners, Grow, Item, Part, Shape};

/// Most columns that each carry their value; past this a column chart gets
/// a light axis and writes only the named values.
const MAX_LABELLED_COLUMNS: usize = 8;

/// Ticks on that axis, as the gallery's column chart asks for.
const AXIS_TICKS: f32 = 4.0;

/// Vertical bars from zero, values above (playbook M1, M2).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn columns(c: &mut Ctx) -> f32 {
    let pts = c.categories("y");
    let values: Vec<f64> = pts.iter().map(|p| p.value).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let (lo, hi) = nice(
        values.iter().copied().fold(0.0, f64::min),
        values.iter().copied().fold(0.0, f64::max),
        10.0,
    );
    // A few columns each carry their value; a long run gets a light axis and
    // only the named values, as the gallery's twelve-month column does.
    let axis = pts.len() > MAX_LABELLED_COLUMNS;
    let labelled: Vec<bool> = pts
        .iter()
        .map(|p| !axis || c.is_highlight(&p.label))
        .collect();
    let left = if axis {
        c.left + PAD + c.tick_label_width(lo, hi, AXIS_TICKS) + c.t.marks.label_padding
    } else {
        c.left + PAD
    };
    let band = Band::new(
        pts.len(),
        left,
        c.left + c.width - PAD,
        c.t.marks.band_padding_inner,
        c.t.marks.band_padding_outer,
    );
    let vs = c.value_style();
    let h = c.t.marks.plot_height;
    let off = c.t.marks.value_offset;
    let probe = Linear {
        d0: lo,
        d1: hi,
        r0: h,
        r1: 0.0,
    };
    // Room above the plot for the tallest labelled bar's value, and for the
    // top tick label when there is an axis.
    let mut label_top = values
        .iter()
        .zip(&labelled)
        .filter(|(_, l)| **l)
        .map(|(v, _)| probe.map(v.max(0.0)) - off - vs.size * 0.5)
        .fold(0.0, f32::min);
    if axis {
        label_top = label_top.min(-c.label_style().size * 0.5);
    }
    // Vega rounds the room it keeps for marks up to whole pixels, and its
    // text box runs a pixel past the font size (measured: tests/reference.rs).
    let top = c.top + PAD + (1.0 - label_top).ceil();
    let y = Linear {
        d0: lo,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    let zero = y.map(0.0);
    let right = band.at(pts.len().saturating_sub(1)) + band.width;
    if axis {
        c.y_axis(&y, Some(AXIS_TICKS), band.start, right);
    }
    let mut named: Option<(Rect, Rect)> = None;
    let mut tops = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let hl = c.is_highlight(&p.label);
        let (e, col) = c.mark_color(hl);
        let (x0, w) = (band.at(i), band.width);
        let yv = y.map(p.value);
        let (rect, corners) = if p.value >= 0.0 {
            (Rect::from_ltrb(x0, yv, x0 + w, zero), Corners::Top)
        } else {
            (Rect::from_ltrb(x0, zero, x0 + w, yv), Corners::Bottom)
        };
        tops.push(rect);
        let first = hl && named.is_none();
        if first {
            named = Some((rect, rect));
        }
        let tip = c.tip(&p.rows, &nf);
        let shape = Shape::Rect {
            rect,
            radius: c.t.marks.bar_radius,
            corners,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(p.rows[0], tip)
                .highlighted(hl)
                .grow(Grow::FromY(zero)),
        );
        if !labelled[i] {
            continue;
        }
        let center = if p.value >= 0.0 { yv - off } else { yv + off };
        let vc = c.value_color(hl);
        let fade = c.items.len();
        c.text(
            Part::Value,
            nf.format(p.value),
            band.center(i),
            center + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Middle,
            vc,
        );
        c.items[fade].grow = Grow::Fade;
        let value = c.text_box(&c.items[fade]);
        if let (true, Some((_, own)), Some(b)) = (first, named.as_mut(), value) {
            own.join(b);
        }
    }
    let bottom = y.map(lo);
    c.plot = Rect::from_ltrb(band.start, top, right, bottom);
    if let (Some(note), Some(bar)) = (c.intent.note.clone(), named) {
        column_note(c, &note, bar, &tops, c.plot);
    }
    let labels: Vec<(String, f32)> = pts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.label.clone(), band.center(i)))
        .collect();
    let below = c.bottom_labels(&labels, bottom, Under::Bands);
    bottom + below + PAD - c.top
}

/// The note beside a named column, as gallery 27 sets "Checkout bug, 4 to
/// 9 Nov": muted, on the side with room, near the column's top and above
/// every column it passes over. `own` is the column with its value.
fn column_note(c: &mut Ctx, note: &str, (bar, own): (Rect, Rect), bars: &[Rect], plot: Rect) {
    let style = c.label_style();
    let gap = c.t.marks.label_padding;
    let spots = |c: &Ctx, w: f32| {
        c.note_spots(w, bar, gap, (plot.left, plot.right))
            .into_iter()
            .map(|(x, anchor)| {
                let r = text_rect(w, x, 0.0, style, anchor);
                let mut baseline = bar.top + style.size + 4.0;
                let crossed = bars
                    .iter()
                    .filter(|b| b.right > r.left && b.left < r.right)
                    .map(|b| b.top)
                    .fold(f32::MAX, f32::min);
                if baseline + 4.0 > crossed {
                    baseline = crossed - 6.0;
                }
                (x, anchor, baseline)
            })
            .collect()
    };
    let rows = (
        plot.top + style.size,
        plot.bottom - (0.2 * style.size).ceil(),
    );
    c.note(note, spots, rows, bars, (own, plot.left));
}
