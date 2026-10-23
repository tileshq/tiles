//! The slope chart.

use skia_safe::Rect;

use super::{
    Ctx, LABEL_SHARE, MIDDLE, PAD, TOP, cells, right_to_left, series_colors, spread, symbol_radius,
};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice};
use crate::scene::{Anchor, Emphasis, Grow, Item, Part, Shape};

/// Each series' change between the first and last period.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn slope(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let series = c.distinct("series");
    let grid = cells(c, &xs, &series);
    let (first, last) = (0, xs.len() - 1);
    let values: Vec<f64> = [first, last]
        .iter()
        .flat_map(|&xi| grid[xi].iter().flatten().map(|(v, _)| *v))
        .collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let (colors, series_hl) = series_colors(c, &series, false);
    let vs = c.value_style();
    let ls = c.label_style();
    // The gallery's slope names each series once, on the left with its
    // first value, and writes only the last value on the right, firmer.
    let first_style = vs.with_weight(400);
    // A spotlight on one line draws it firm with larger end dots and its
    // labels at weight 600, and the rest thin, as multi-line does (M4, M8).
    let spotlight = series_hl.iter().any(|h| *h);
    let style_of = |si: usize, left_side: bool| {
        if series_hl[si] {
            vs.with_weight(600)
        } else if left_side {
            first_style
        } else {
            vs
        }
    };
    let shown = c.fit_all(&series, first_style, LABEL_SHARE / 2.0);
    // The space between a left label's name and its value.
    let apart = "  ";
    let label = |s: &str, v: f64, left_side: bool| {
        if left_side {
            format!("{s}{apart}{}", nf.format(v))
        } else {
            nf.format(v)
        }
    };
    let side_w = |xi: usize, left_side: bool| {
        shown
            .iter()
            .enumerate()
            .filter_map(|(si, s)| {
                grid[xi][si]
                    .as_ref()
                    .map(|(v, _)| c.width_of(&label(s, *v, left_side), style_of(si, left_side)))
            })
            .fold(0.0, f32::max)
    };
    let gap = 10.0;
    let left = c.left + PAD + side_w(first, true) + gap;
    let right = c.left + c.width - PAD - side_w(last, false) - gap;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = nice(min, max, 10.0);
    let h = c.t.marks.plot_height;
    let top = c.top + PAD + ls.size + 10.0;
    let y = Linear {
        d0: lo,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    let muted = (Emphasis::Muted, c.pal.muted);
    // The two periods head their columns in ink, as the gallery sets them.
    let head = c.top + PAD + (TOP * ls.size).round();
    let ink = (Emphasis::Ink, c.pal.ink);
    for (xi, x) in [(first, left), (last, right)] {
        c.text(
            Part::AxisLabel,
            xs[xi].clone(),
            x,
            head,
            ls.with_weight(600),
            Anchor::Middle,
            ink,
        );
    }
    let mut order: Vec<usize> = (0..series.len()).collect();
    order.sort_by_key(|&i| series_hl[i]);
    let mut left_labels = Vec::new();
    let mut right_labels = Vec::new();
    for &si in &order {
        let (Some((a, ra)), Some((b, rb))) = (&grid[first][si], &grid[last][si]) else {
            continue;
        };
        let (e, col) = colors[si];
        let (ya, yb) = (y.map(*a), y.map(*b));
        let mut rows = ra.clone();
        rows.extend(rb);
        let tip = format!(
            "{}\n{}: {} → {}",
            series[si],
            c.intent.fields.get("y").cloned().unwrap_or_default(),
            nf.format(*a),
            nf.format(*b)
        );
        let (width, r) = match (spotlight, series_hl[si]) {
            (true, true) => (c.t.marks.line_width_named, c.t.marks.point_radius_highlight),
            (true, false) => (c.t.marks.line_width_rest, c.t.marks.point_radius),
            _ => (c.t.marks.line_width, symbol_radius(60.0)),
        };
        let shape = Shape::Line {
            points: vec![(left, ya), (right, yb)],
            width,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(rows[0], tip.clone())
                .highlighted(series_hl[si])
                .grow(Grow::Wipe),
        );
        for (x, yy) in [(left, ya), (right, yb)] {
            let shape = Shape::Circle { cx: x, cy: yy, r };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(rows[0], tip.clone())
                    .highlighted(series_hl[si])
                    .grow(Grow::Wipe),
            );
        }
        left_labels.push((si, ya, *a));
        right_labels.push((si, yb, *b));
    }
    for (labels, left_side) in [(left_labels, true), (right_labels, false)] {
        let mut ys: Vec<(usize, f32)> = labels.iter().map(|(si, yy, _)| (*si, *yy)).collect();
        spread(&mut ys, vs.size + 2.0, (top, top + h));
        for (si, yy) in ys {
            let v = labels.iter().find(|l| l.0 == si).map_or(0.0, |l| l.2);
            let hl = series_hl[si];
            let color = if hl || !c.any_highlight() {
                colors[si]
            } else {
                muted
            };
            let (x, anchor) = if left_side {
                (left - gap, Anchor::End)
            } else {
                (right + gap, Anchor::Start)
            };
            let at = c.items.len();
            let style = style_of(si, left_side);
            let baseline = yy + (MIDDLE * style.size).round();
            let value = nf.format(v);
            // A right-to-left name is set apart from its value: in one run,
            // bidi ordering would put the value on the name's left.
            if left_side && right_to_left(&shown[si]) {
                let name_end = x - c.width_of(&format!("{apart}{value}"), style);
                c.text(
                    Part::Value,
                    shown[si].clone(),
                    name_end,
                    baseline,
                    style,
                    anchor,
                    color,
                );
                c.text(Part::Value, value, x, baseline, style, anchor, color);
            } else {
                c.text(
                    Part::Value,
                    label(&shown[si], v, left_side),
                    x,
                    baseline,
                    style,
                    anchor,
                    color,
                );
            }
            for item in &mut c.items[at..] {
                item.grow = Grow::Fade;
            }
        }
    }
    c.plot = Rect::from_ltrb(left, top, right, top + h);
    top + h + PAD + vs.size * 0.5 - c.top
}
