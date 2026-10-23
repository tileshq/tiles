//! The bullet chart.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The bullet's qualitative bands, as shares of each row's target: under
/// 70%, 70 to 90% and 90 to 115%, as the gallery's bullet draws them.
const BULLET_BANDS: [f64; 3] = [0.7, 0.9, 1.15];

/// The bullet's plot height, as the gallery's bullet sets it.
const BULLET_HEIGHT: f32 = 200.0;

/// A bullet row's band, bar and target tick heights as shares of its band
/// width: the gallery's 0.75 and 0.28 bands, and its tick, which Vega draws
/// the full band tall.
const BULLET_SHARES: (f32, f32, f32) = (0.75, 0.28, 1.0);

/// The target tick's opacity, Vega's default for a tick mark.
const BULLET_TICK_OPACITY: f32 = 0.7;

/// A value against its target per row: a dark bar on three bands that grade
/// the target, a tick at the target, the share of target beside it and a
/// value axis below (playbook M11).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn bullet(c: &mut Ctx) -> f32 {
    let rows: Vec<(Vec<usize>, String, f64, Option<f64>)> = c
        .sums(&["y", "target"], 1)
        .into_iter()
        .map(|s| (s.rows, s.label, s.values[0].unwrap_or(0.0), s.values[1]))
        .collect();
    let values: Vec<f64> = rows
        .iter()
        .flat_map(|r| [Some(r.2), r.3])
        .flatten()
        .collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let vs = c.row_value_style();
    let off = c.t.marks.label_padding;
    let names: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    // The share of target where there is one, else the value.
    let pf = NumberFormat::for_values(&[0.0]);
    let labels: Vec<String> = rows
        .iter()
        .map(|r| match r.3 {
            Some(t) if t > 0.0 => format!("{}%", pf.format((r.2 / t * 100.0).round())),
            _ => nf.format(r.2),
        })
        .collect();
    // Where each row's text starts from: past its bands and its bar.
    let reach: Vec<f64> = rows
        .iter()
        .map(|r| {
            let band =
                r.3.filter(|t| *t > 0.0)
                    .map_or(0.0, |t| t * BULLET_BANDS[2]);
            band.max(r.2).max(0.0)
        })
        .collect();
    let lo = values.iter().copied().fold(0.0, f64::min);
    let hi = reach.iter().copied().fold(0.0, f64::max);
    let (lo, hi) = nice(lo, hi.max(values.iter().copied().fold(0.0, f64::max)), 4.0);
    let left = c.left + PAD + widest + c.t.marks.label_padding;
    let limit = c.left + c.width - PAD;
    let tf = c.tick_format(&ticks(lo, hi, 4.0), true);
    // The scale narrows only as far as a row's text past its reach, or the
    // last tick's half width, needs.
    let label_w = |l: &str| c.width_of(l, vs);
    let right = reach
        .iter()
        .zip(&labels)
        .fold(
            limit - c.width_of(&tf.format(hi), ls) / 2.0,
            |right, (r, l)| {
                #[allow(clippy::cast_possible_truncation)] // A share of the scale, 0 to 1.
                let t = ((r - lo) / (hi - lo)) as f32;
                if t > 0.0 {
                    right.min(left + (limit - left - off - label_w(l)) / t)
                } else {
                    right
                }
            },
        )
        .max(left + 1.0);
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let inner = c.t.marks.band_padding_inner;
    let outer = c.t.marks.band_padding_outer;
    let span_of = |step: f32| step * (f(rows.len()) - inner + 2.0 * outer);
    let step = c.t.marks.row_step.max(BULLET_HEIGHT / span_of(1.0));
    let top = c.top + PAD;
    let span = span_of(step);
    let band = Band::new(rows.len(), top, top + span, inner, outer);
    let base = x.map(0.0_f64.clamp(lo, hi));
    let (band_share, bar_share, tick_share) = BULLET_SHARES;
    let shades = [
        (Emphasis::Neutral(2), c.pal.neutrals[2]),
        (Emphasis::Track, c.pal.track),
        (Emphasis::Grid, c.pal.grid.with_opacity(0.5)),
    ];
    for (i, (r, label, v, target)) in rows.iter().enumerate() {
        let cy = band.center(i);
        let hl = c.is_highlight(label);
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            left - c.t.marks.label_padding,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            muted,
        );
        let target = target.filter(|t| *t > 0.0);
        let band_h = band.width * band_share;
        if let Some(t) = target {
            let mut from = 0.0;
            for (share, (e, col)) in BULLET_BANDS.iter().zip(shades) {
                let to = t * share;
                let rect =
                    Rect::from_ltrb(x.map(from), cy - band_h / 2.0, x.map(to), cy + band_h / 2.0);
                let shape = Shape::Rect {
                    rect,
                    radius: 0.0,
                    corners: Corners::All,
                };
                c.items.push(Item::new(Part::Guide, shape, e, col));
                from = to;
            }
        }
        // The named rows take the accent, or red for a bad finding; the
        // rest are dark on their bands, as the gallery's bullet draws them.
        let (e, col) = if !c.any_highlight() || hl {
            c.mark_color(hl)
        } else {
            (Emphasis::Muted, c.pal.muted)
        };
        let bar_h = band.width * bar_share;
        let rect = Rect::from_ltrb(
            base.min(x.map(*v)),
            cy - bar_h / 2.0,
            base.max(x.map(*v)),
            cy + bar_h / 2.0,
        );
        let tip = c.tip(r, &nf);
        let shape = Shape::Rect {
            rect,
            radius: c.t.marks.bar_radius,
            corners: Corners::Right,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(r[0], tip)
                .highlighted(hl)
                .grow(Grow::FromX(base)),
        );
        if let Some(t) = target {
            let tx = x.map(t);
            let tick_h = band.width * tick_share;
            let shape = Shape::Line {
                points: vec![(tx, cy - tick_h / 2.0), (tx, cy + tick_h / 2.0)],
                width: 2.0,
            };
            // Ink at Vega's tick opacity, as the gallery's tick is.
            let ink = c.pal.ink.with_opacity(BULLET_TICK_OPACITY);
            c.items
                .push(Item::new(Part::Guide, shape, Emphasis::Ink, ink).grow(Grow::Fade));
        }
        let at = c.items.len();
        let vc = c.value_color(hl);
        c.text(
            Part::Value,
            labels[i].clone(),
            x.map(reach[i]) + off,
            cy + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Start,
            vc,
        );
        c.items[at].grow = Grow::Fade;
    }
    let bottom = top + span;
    let tick_labels: Vec<(String, f32)> = ticks(lo, hi, 4.0)
        .into_iter()
        .map(|v| (tf.format(v), x.map(v)))
        .collect();
    let below = c.bottom_labels(&tick_labels, bottom, Under::Values);
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    span + below + 2.0 * PAD
}
