//! Stacked columns.

use skia_safe::Rect;

use super::{Ctx, MIDDLE, PAD, Under, cells, series_colors};
use crate::color::Rgba;
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Columns stacked from zero, totals on top.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn stacked(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let series = c.distinct("series");
    let grid = cells(c, &xs, &series);
    let totals: Vec<f64> = grid
        .iter()
        .map(|row| row.iter().flatten().map(|(v, _)| *v).sum())
        .collect();
    let nf = NumberFormat::for_values(&totals).with_unit(c.intent.unit);
    let (colors, series_hl) = series_colors(c, &series, true);
    let entries: Vec<(String, (Emphasis, Rgba))> =
        series.iter().cloned().zip(colors.iter().copied()).collect();
    let legend = c.legend(&entries, c.top);
    let (_, hi) = nice(0.0, totals.iter().copied().fold(0.0, f64::max), 10.0);
    let band = Band::new(
        xs.len(),
        c.left + PAD,
        c.left + c.width - PAD,
        c.t.marks.band_padding_inner,
        c.t.marks.band_padding_outer,
    );
    let vs = c.value_style();
    let h = c.t.marks.plot_height;
    let off = c.t.marks.value_offset;
    let top = c.top + PAD + legend + vs.size + off;
    let y = Linear {
        d0: 0.0,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    let zero = y.map(0.0);
    for (xi, row) in grid.iter().enumerate() {
        let mut acc = 0.0;
        let last = row
            .iter()
            .rposition(|cell| cell.as_ref().is_some_and(|(v, _)| *v > 0.0));
        for (si, cell) in row.iter().enumerate() {
            let Some((v, rows)) = cell else { continue };
            if *v <= 0.0 {
                continue;
            }
            let (y0, y1) = (y.map(acc), y.map(acc + v));
            acc += v;
            let (e, col) = colors[si];
            let corners = if Some(si) == last {
                Corners::Top
            } else {
                Corners::All
            };
            let radius = if Some(si) == last {
                c.t.marks.bar_radius
            } else {
                0.0
            };
            let rect = Rect::from_ltrb(band.at(xi), y1, band.at(xi) + band.width, y0 - 0.5);
            let tip = c.tip(rows, &nf);
            let shape = Shape::Rect {
                rect,
                radius,
                corners,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(rows[0], tip)
                    .highlighted(series_hl[si])
                    .grow(Grow::FromY(zero)),
            );
        }
        let muted = (Emphasis::Muted, c.pal.muted);
        let at = c.items.len();
        let cy = y.map(totals[xi]) - off;
        c.text(
            Part::Value,
            nf.format(totals[xi]),
            band.center(xi),
            cy + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Middle,
            muted,
        );
        c.items[at].grow = Grow::Fade;
    }
    let bottom = zero;
    c.plot = Rect::from_ltrb(band.start, top, band.at(xs.len() - 1) + band.width, bottom);
    let labels: Vec<(String, f32)> = xs
        .iter()
        .enumerate()
        .map(|(i, l)| (l.clone(), band.center(i)))
        .collect();
    let below = c.bottom_labels(&labels, bottom, Under::Bands);
    bottom + below + PAD - c.top
}
