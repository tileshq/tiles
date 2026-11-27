//! The diverging bar: signed bars either side of a zero rule.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, f};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The plot height, as the gallery's diverging bars set it.
const HEIGHT: f32 = 220.0;

/// Most room one bar takes, so a few bars stay bars.
const MAX_STEP: f32 = 40.0;

/// Signed bars from a zero rule, largest first, each value written with its
/// sign past its bar's end: right of a bar above zero, left of one below
/// (playbook M1, M2). The named bars take the accent (red for a bad
/// finding) and the rest are grey.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn diverging(c: &mut Ctx) -> f32 {
    let pts = c.categories("y");
    let values: Vec<f64> = pts.iter().map(|p| p.value).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let vs = c.row_value_style();
    let off = c.t.marks.row_value_offset;
    let names: Vec<String> = pts.iter().map(|p| p.label.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    let labels: Vec<String> = values.iter().map(|v| nf.signed(*v)).collect();
    let room = |below: bool| {
        values
            .iter()
            .zip(&labels)
            .filter(|(v, _)| (**v < 0.0) == below)
            .map(|(_, l)| c.width_of(l, vs) + off)
            .fold(0.0, f32::max)
    };
    // The bars carry their values and draw no ticks, so the scale runs
    // from the lowest value to the highest, zero included, with no nice
    // ends to waste the card's width.
    let lo = values.iter().copied().fold(0.0, f64::min);
    let hi = values.iter().copied().fold(0.0, f64::max);
    let (lo, hi) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
    let label_edge = c.left + PAD + widest;
    let left = label_edge + c.t.marks.label_padding + room(true);
    let right = (c.left + c.width - PAD - room(false)).max(left + 1.0);
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let inner = c.t.marks.band_padding_inner;
    let outer = c.t.marks.band_padding_outer;
    let span_of = |step: f32| step * (f(pts.len()) - inner + 2.0 * outer);
    let step =
        c.t.marks
            .row_step
            .max((HEIGHT / span_of(1.0)).min(MAX_STEP));
    let top = c.top + PAD;
    let span = span_of(step);
    let band = Band::new(pts.len(), top, top + span, inner, outer);
    let zero = x.map(0.0);
    let thick = band.width * c.t.marks.bar_band;
    for (i, p) in pts.iter().enumerate() {
        let cy = band.center(i);
        let hl = c.is_highlight(&p.label);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            label_edge,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            (Emphasis::Muted, c.pal.muted),
        );
        let (e, col) = c.mark_color(hl);
        let xv = x.map(p.value);
        let (rect, corners) = if p.value >= 0.0 {
            (
                Rect::from_ltrb(zero, cy - thick / 2.0, xv, cy + thick / 2.0),
                Corners::Right,
            )
        } else {
            (
                Rect::from_ltrb(xv, cy - thick / 2.0, zero, cy + thick / 2.0),
                Corners::Left,
            )
        };
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
                .grow(Grow::FromX(zero)),
        );
        let (lx, anchor) = if p.value >= 0.0 {
            (xv.max(zero) + off, Anchor::Start)
        } else {
            (xv - off, Anchor::End)
        };
        let at = c.items.len();
        let vc = c.row_value_color(hl);
        c.text(
            Part::Value,
            labels[i].clone(),
            lx,
            cy + (MIDDLE * vs.size).round(),
            vs,
            anchor,
            vc,
        );
        c.items[at].grow = Grow::Fade;
    }
    // The zero rule over the bars' bases, as the gallery draws it.
    let rule = Shape::Line {
        points: vec![(zero, top), (zero, top + span)],
        width: 1.0,
    };
    c.items
        .push(Item::new(Part::Guide, rule, Emphasis::Muted, c.pal.muted));
    c.plot = Rect::from_ltrb(left, top, right, top + span);
    span + 2.0 * PAD
}
