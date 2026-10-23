//! The scatter plot.

use skia_safe::Rect;

use super::{Ctx, MIDDLE, PAD, Under, symbol_radius};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, ticks};
use crate::scene::{Anchor, Grow, Item, Part, Shape};

/// Points on two measures; highlighted items are labelled.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn scatter(c: &mut Ctx) -> f32 {
    let (xc, yc) = (c.col("x").unwrap_or(0), c.col("y").unwrap_or(0));
    let lc = c.col("label");
    let pts: Vec<(usize, f64, f64, String)> = c
        .intent
        .table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(r, row)| {
            Some((
                r,
                row[xc].number()?,
                row[yc].number()?,
                lc.map(|l| row[l].label()).unwrap_or_default(),
            ))
        })
        .collect();
    let xv: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let yv: Vec<f64> = pts.iter().map(|p| p.2).collect();
    let both = NumberFormat::for_values(&[xv.clone(), yv.clone()].concat());
    let fold = |v: &[f64]| {
        (
            v.iter().copied().fold(f64::INFINITY, f64::min),
            v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let ((x0, x1), (y0, y1)) = (fold(&xv), fold(&yv));
    // Pad each domain so no point sits on the plot's edge, beside the axis
    // labels, without inventing a negative range for non-negative data.
    let padded = |lo: f64, hi: f64| {
        let span = if hi > lo { hi - lo } else { lo.abs().max(1.0) };
        let (plo, phi) = (lo - span * 0.06, hi + span * 0.06);
        nice(if lo >= 0.0 { plo.max(0.0) } else { plo }, phi, 10.0)
    };
    let (xlo, xhi) = padded(x0, x1);
    let (ylo, yhi) = padded(y0, y1);
    let h = c.t.marks.plot_height;
    let count = (h / c.t.marks.tick_spacing).ceil();
    let tick_w = c.tick_label_width(ylo, yhi, count);
    let ls = c.label_style();
    let left = c.left + PAD + tick_w + c.t.marks.label_padding;
    // Room for the last x tick label, written as the ticks will be.
    let xf = c.tick_format(&ticks(xlo, xhi, 6.0), false);
    let right = c.left + c.width - PAD - c.width_of(&xf.format(xhi), ls) / 2.0;
    let top = c.top + PAD + (ls.size * 0.5).ceil();
    let y = Linear {
        d0: ylo,
        d1: yhi,
        r0: top + h,
        r1: top,
    };
    let x = Linear {
        d0: xlo,
        d1: xhi,
        r0: left,
        r1: right,
    };
    c.y_axis(&y, None, left, right);
    let xcount = ((right - left) / 80.0).ceil();
    let xt = ticks(xlo, xhi, f64::from(xcount));
    let xf = c.tick_format(&xt, false);
    let labels: Vec<(String, f32)> = xt.iter().map(|v| (xf.format(*v), x.map(*v))).collect();
    let mut order: Vec<usize> = (0..pts.len()).collect();
    order.sort_by_key(|&i| c.is_highlight(&pts[i].3));
    let r = symbol_radius(70.0);
    for i in order {
        let (row, a, b, label) = &pts[i];
        let hl = !label.is_empty() && c.is_highlight(label);
        let (e, col) = c.mark_color(hl);
        let alpha = if c.any_highlight() && !hl { 1.0 } else { 0.85 };
        let tip = c.tip_with(&[*row], |role| {
            if role == "y" {
                both.with_unit(c.intent.unit)
            } else {
                both
            }
        });
        let (cx, cy) = (x.map(*a), y.map(*b));
        let shape = Shape::Circle { cx, cy, r };
        c.items.push(
            Item::new(Part::Mark, shape, e, col.with_opacity(alpha))
                .datum(*row, tip)
                .highlighted(hl)
                .grow(Grow::Fade),
        );
        if hl {
            let vs = c.value_style();
            let at = c.items.len();
            // Labels sit right of the point unless that would leave the plot.
            let room = right - cx - r - 5.0;
            let (lx, anchor) = if c.width_of(label, vs) <= room {
                (cx + r + 5.0, Anchor::Start)
            } else {
                (cx - r - 5.0, Anchor::End)
            };
            // Keep the label inside the plot, clear of the axis labels.
            let ly = cy.clamp(top + vs.size * 0.6, top + h - vs.size * 0.6);
            c.text(
                Part::Value,
                label.clone(),
                lx,
                ly + (MIDDLE * vs.size).round(),
                vs,
                anchor,
                (e, col),
            );
            c.items[at].grow = Grow::Fade;
        }
    }
    let bottom = top + h;
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    let below = c.bottom_labels(&labels, bottom, Under::Values);
    bottom + below + PAD - c.top
}
