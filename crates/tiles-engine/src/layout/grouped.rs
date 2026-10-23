//! Grouped columns.

use skia_safe::Rect;

use super::{Ctx, MIDDLE, PAD, Under, cells, f, series_colors};
use crate::color::Rgba;
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Columns side by side per category, one per series.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn grouped(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let series = c.distinct("series");
    let grid = cells(c, &xs, &series);
    let values: Vec<f64> = grid.iter().flatten().flatten().map(|(v, _)| *v).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let (colors, series_hl) = series_colors(c, &series, true);
    let x_hl_any = !series_hl.iter().any(|b| *b) && c.any_highlight();
    let entries: Vec<(String, (Emphasis, Rgba))> =
        series.iter().cloned().zip(colors.iter().copied()).collect();
    let legend = c.legend(&entries, c.top);
    let (lo, hi) = nice(
        values.iter().copied().fold(0.0, f64::min),
        values.iter().copied().fold(0.0, f64::max),
        10.0,
    );
    let band = Band::new(
        xs.len(),
        c.left + PAD,
        c.left + c.width - PAD,
        0.2,
        c.t.marks.band_padding_outer,
    );
    let sub_w = band.width / f(series.len().max(1));
    let vs = c.value_style();
    let fits = values
        .iter()
        .all(|v| c.width_of(&nf.format(*v), vs) <= sub_w + 2.0);
    let h = c.t.marks.plot_height;
    let off = c.t.marks.value_offset;
    let top = c.top
        + PAD
        + legend
        + if fits {
            vs.size + off
        } else {
            c.label_style().size * 0.5
        };
    let y = Linear {
        d0: lo,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    let zero = y.map(0.0);
    if !fits {
        c.y_axis(&y, None, band.start, band.at(xs.len() - 1) + band.width);
    }
    for (xi, label) in xs.iter().enumerate() {
        for (si, cell) in grid[xi].iter().enumerate() {
            let Some((v, rows)) = cell else { continue };
            let x0 = band.at(xi) + sub_w * f(si);
            let hl = series_hl[si] || (x_hl_any && c.is_highlight(label));
            let (e, col) = if x_hl_any && !c.is_highlight(label) {
                (Emphasis::Grey, c.pal.grey)
            } else {
                colors[si]
            };
            let yv = y.map(*v);
            let (rect, corners) = if *v >= 0.0 {
                (
                    Rect::from_ltrb(x0 + 0.5, yv, x0 + sub_w - 0.5, zero),
                    Corners::Top,
                )
            } else {
                (
                    Rect::from_ltrb(x0 + 0.5, zero, x0 + sub_w - 0.5, yv),
                    Corners::Bottom,
                )
            };
            let tip = c.tip(rows, &nf);
            let shape = Shape::Rect {
                rect,
                radius: c.t.marks.bar_radius,
                corners,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(rows[0], tip)
                    .highlighted(hl)
                    .grow(Grow::FromY(zero)),
            );
            if fits {
                let vc = if hl {
                    (e, col)
                } else {
                    (Emphasis::Muted, c.pal.muted)
                };
                let cy = if *v >= 0.0 { yv - off } else { yv + off };
                let at = c.items.len();
                c.text(
                    Part::Value,
                    nf.format(*v),
                    x0 + sub_w / 2.0,
                    cy + (MIDDLE * vs.size).round(),
                    vs,
                    Anchor::Middle,
                    vc,
                );
                c.items[at].grow = Grow::Fade;
            }
        }
    }
    let bottom = y.map(lo);
    c.plot = Rect::from_ltrb(band.start, top, band.at(xs.len() - 1) + band.width, bottom);
    let labels: Vec<(String, f32)> = xs
        .iter()
        .enumerate()
        .map(|(i, l)| (l.clone(), band.center(i)))
        .collect();
    let below = c.bottom_labels(&labels, bottom, Under::Bands);
    bottom + below + PAD - c.top
}
