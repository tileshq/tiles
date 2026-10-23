//! The waterfall chart.

use skia_safe::Rect;

use super::{Ctx, MIDDLE, PAD, Under};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Signed steps from zero to a total.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn waterfall(c: &mut Ctx) -> f32 {
    let pts = c.categories("y");
    let mut steps: Vec<(f64, f64)> = Vec::new();
    let mut acc = 0.0;
    for p in &pts {
        steps.push((acc, acc + p.value));
        acc += p.value;
    }
    let total = acc;
    let mut all: Vec<f64> = steps.iter().flat_map(|(a, b)| [*a, *b]).collect();
    all.push(0.0);
    let deltas: Vec<f64> = pts.iter().map(|p| p.value).chain([total]).collect();
    let nf = NumberFormat::for_values(&deltas).with_unit(c.intent.unit);
    let (lo, hi) = nice(
        all.iter().copied().fold(0.0, f64::min),
        all.iter().copied().fold(0.0, f64::max),
        10.0,
    );
    let n = pts.len() + 1;
    let band = Band::new(
        n,
        c.left + PAD,
        c.left + c.width - PAD,
        c.t.marks.band_padding_inner + 0.1,
        c.t.marks.band_padding_outer,
    );
    let vs = c.value_style();
    let off = c.t.marks.value_offset;
    let h = c.t.marks.plot_height;
    let top = c.top + PAD + vs.size + off;
    let y = Linear {
        d0: lo,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    let zero = y.map(0.0);
    let any = c.any_highlight();
    for (i, p) in pts.iter().enumerate() {
        let (a, b) = steps[i];
        let hl = c.is_highlight(&p.label);
        let rise = b >= a;
        let (e, col) = if any && !hl {
            (Emphasis::Grey, c.pal.grey)
        } else if rise {
            (Emphasis::Accent, c.pal.accent)
        } else {
            (Emphasis::Bad, c.pal.bad)
        };
        let (ya, yb) = (y.map(a), y.map(b));
        let rect = Rect::from_ltrb(
            band.at(i),
            ya.min(yb),
            band.at(i) + band.width,
            ya.max(yb).max(ya.min(yb) + 1.0),
        );
        let tip = c.tip(&p.rows, &nf);
        let shape = Shape::Rect {
            rect,
            radius: c.t.marks.bar_radius,
            corners: if rise { Corners::Top } else { Corners::Bottom },
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(p.rows[0], tip)
                .highlighted(hl)
                .grow(Grow::FromY(ya)),
        );
        let vc = if any && !hl {
            (Emphasis::Muted, c.pal.muted)
        } else {
            (e, col)
        };
        let cy = if rise { yb - off } else { yb + off };
        let at = c.items.len();
        c.text(
            Part::Value,
            nf.signed(p.value),
            band.center(i),
            cy + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Middle,
            vc,
        );
        c.items[at].grow = Grow::Fade;
        let next = band.at(i + 1);
        let guide = Shape::Line {
            points: vec![(band.at(i) + band.width, yb), (next, yb)],
            width: 1.0,
        };
        c.items.push(
            Item::new(
                Part::Guide,
                guide,
                Emphasis::Grid,
                c.pal.muted.with_opacity(0.5),
            )
            .grow(Grow::Fade),
        );
    }
    let i = pts.len();
    let yt = y.map(total);
    let rect = Rect::from_ltrb(
        band.at(i),
        yt.min(zero),
        band.at(i) + band.width,
        yt.max(zero),
    );
    let corners = if total >= 0.0 {
        Corners::Top
    } else {
        Corners::Bottom
    };
    let tip = format!("Total: {}", nf.format(total));
    let shape = Shape::Rect {
        rect,
        radius: c.t.marks.bar_radius,
        corners,
    };
    let mut item =
        Item::new(Part::Mark, shape, Emphasis::Muted, c.pal.muted).grow(Grow::FromY(zero));
    item.tip = Some(tip);
    c.items.push(item);
    let cy = if total >= 0.0 { yt - off } else { yt + off };
    let ink = (Emphasis::Ink, c.pal.ink);
    let at = c.items.len();
    c.text(
        Part::Value,
        nf.format(total),
        band.center(i),
        cy + (MIDDLE * vs.size).round(),
        vs,
        Anchor::Middle,
        ink,
    );
    c.items[at].grow = Grow::Fade;
    if lo < 0.0 {
        let line = Shape::Line {
            points: vec![(band.start, zero), (band.at(i) + band.width, zero)],
            width: 1.0,
        };
        c.items
            .push(Item::new(Part::Guide, line, Emphasis::Grid, c.pal.grid));
    }
    let bottom = y.map(lo);
    c.plot = Rect::from_ltrb(band.start, top, band.at(i) + band.width, bottom);
    let mut labels: Vec<(String, f32)> = pts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.label.clone(), band.center(i)))
        .collect();
    labels.push(("Total".to_string(), band.center(i)));
    let below = c.bottom_labels(&labels, bottom, Under::Bands);
    bottom + below + PAD - c.top
}
