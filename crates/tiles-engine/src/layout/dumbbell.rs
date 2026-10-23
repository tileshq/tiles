//! The dumbbell chart.

use skia_safe::Rect;
use tiles_intent::Tone;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f, symbol_radius};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, ticks};
use crate::scene::{Anchor, Emphasis, Grow, Item, Part, Shape};

/// Two values per row joined by a bar, the named exception first and then
/// the largest gap, over a value axis with a gridline per row; the periods
/// are named once over one row's dots (playbook M10).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn dumbbell(c: &mut Ctx) -> f32 {
    let ls = c.label_style();
    let mut rows: Vec<(Vec<usize>, String, f64, f64)> = c
        .sums(&["lo", "hi"], 2)
        .into_iter()
        .map(|s| {
            let (a, b) = (s.values[0].unwrap_or(0.0), s.values[1].unwrap_or(0.0));
            (s.rows, s.label, a, b)
        })
        .collect();
    // The row the title names comes first (the exception the gallery's
    // dumbbell leads with), then the largest gap.
    rows.sort_by(|a, b| {
        c.is_highlight(&b.1)
            .cmp(&c.is_highlight(&a.1))
            .then((b.3 - b.2).abs().total_cmp(&(a.3 - a.2).abs()))
    });
    let names: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let values: Vec<f64> = rows.iter().flat_map(|r| [r.2, r.3]).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let vs = c.value_style();
    let dot = symbol_radius(DUMBBELL_DOT);
    let off = c.t.marks.label_padding;
    let periods = [
        c.intent.fields.get("lo").cloned().unwrap_or_default(),
        c.intent.fields.get("hi").cloned().unwrap_or_default(),
    ];
    // The first value is where things stood, in grey; the second is the
    // finding, in the accent. A bad finding's named rows are the exception,
    // in red on a pale red bar; any other finding's named rows lead and the
    // rest fade.
    let bad = c.intent.claim.tone == Tone::Bad;
    let lo_color = (Emphasis::Grey, c.pal.grey);
    let hi_color = (Emphasis::Accent, c.pal.accent);
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // Times and counts read from zero, as the gallery's minutes do, unless
    // the values sit far from it.
    let from = if min >= 0.0 && min <= max / 2.0 {
        0.0
    } else {
        min
    };
    let (lo, hi) = nice(from, max, DUMBBELL_TICKS);
    let tf = c.tick_format(&ticks(lo, hi, DUMBBELL_TICKS), true);
    // Each period is named once, over the dots of the first accented row
    // that leaves room between them; with no such row, a legend.
    let name_w = |k: usize| c.width_of(&periods[k], ls);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    let left = c.left + PAD + widest + c.t.marks.label_padding + dot;
    let right = c.left + c.width - PAD - c.width_of(&tf.format(hi), ls) / 2.0;
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let named_row = rows.iter().position(|r| {
        let hl = c.is_highlight(&r.1);
        // A row whose second dot takes the accent, so each name reads with
        // its colour: beside a bad finding's exception, the rest; else the
        // named rows, or any row when none is named.
        let accented = if bad { !hl } else { hl || !c.any_highlight() };
        let (xa, xb) = (x.map(r.2), x.map(r.3));
        let on_card = [(xa, name_w(0)), (xb, name_w(1))]
            .iter()
            .all(|(xx, w)| xx - w / 2.0 >= c.left + PAD && xx + w / 2.0 <= c.left + c.width - PAD);
        accented && on_card && (xb - xa).abs() >= f32::midpoint(name_w(0), name_w(1)) + off
    });
    let legend = if named_row.is_none() {
        let (e, col) = if bad && rows.iter().all(|r| c.is_highlight(&r.1)) {
            (Emphasis::Bad, c.pal.bad)
        } else {
            hi_color
        };
        c.legend(
            &[
                (periods[0].clone(), lo_color),
                (periods[1].clone(), (e, col)),
            ],
            c.top,
        )
    } else {
        0.0
    };
    let note = c.intent.note.clone();
    let top = c.top + PAD + legend;
    let step = (c.t.marks.row_step + 4.0).max(DUMBBELL_HEIGHT / f(rows.len()));
    let span = step * f(rows.len());
    let mut note_at = None;
    for i in 0..rows.len() {
        let cy = top + step * (f(i) + 0.5);
        let grid = Shape::Line {
            points: vec![(left - dot, cy), (right + dot, cy)],
            width: c.t.marks.grid_width,
        };
        c.items
            .push(Item::new(Part::Grid, grid, Emphasis::Grid, c.pal.grid));
    }
    for (i, (r, label, a, b)) in rows.iter().enumerate() {
        let cy = top + step * (f(i) + 0.5);
        let hl = c.is_highlight(label);
        let dim = c.any_highlight() && !hl && !bad;
        let exception = hl && bad;
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            c.left + PAD + widest,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            muted,
        );
        let (xa, xb) = (x.map(*a), x.map(*b));
        let tip = c.tip(r, &nf);
        let bar_color = if exception {
            (Emphasis::Bad, c.pal.bad_soft)
        } else if dim {
            (Emphasis::Grid, c.pal.grid)
        } else {
            (Emphasis::Grey, c.pal.grey)
        };
        let shape = Shape::Line {
            points: vec![(xa, cy), (xb, cy)],
            width: 3.0,
        };
        c.items
            .push(Item::new(Part::Guide, shape, bar_color.0, bar_color.1).grow(Grow::Fade));
        let b_color = if exception {
            (Emphasis::Bad, c.pal.bad)
        } else if dim {
            (Emphasis::Grey, c.pal.grey)
        } else {
            hi_color
        };
        for (xx, (e, col)) in [(xa, lo_color), (xb, b_color)] {
            // A ring in the card's colour parts a dot from the bar under it.
            let ring = Shape::Circle {
                cx: xx,
                cy,
                r: dot + 0.5,
            };
            c.items
                .push(Item::new(Part::Guide, ring, Emphasis::Card, c.pal.card).grow(Grow::Fade));
            let shape = Shape::Circle {
                cx: xx,
                cy,
                r: dot - 0.5,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(r[0], tip.clone())
                    .highlighted(hl)
                    .grow(Grow::Fade),
            );
        }
        if Some(i) == named_row {
            for (k, xx) in [xa, xb].into_iter().enumerate() {
                let at = c.items.len();
                c.text(
                    Part::Legend,
                    periods[k].clone(),
                    xx,
                    cy - 12.0 + (MIDDLE * ls.size).round(),
                    ls,
                    Anchor::Middle,
                    muted,
                );
                c.items[at].grow = Grow::Fade;
            }
        }
        // The note stands in for the named row's second value, beside its
        // dot on the side away from the first.
        if hl && note.is_some() && note_at.is_none() {
            let (lx, anchor) = if xb <= xa {
                (xb - DUMBBELL_NOTE_DX, Anchor::End)
            } else {
                (xb + DUMBBELL_NOTE_DX, Anchor::Start)
            };
            let baseline = cy + (MIDDLE * vs.size).round();
            let row = Rect::from_ltrb(xa.min(xb) - dot, cy - dot, xa.max(xb) + dot, cy + dot);
            note_at = Some(((nf.format(*b), lx, baseline, anchor), row, cy));
        }
    }
    // The note stands in for the named row's second value where it fits in
    // that row; otherwise it takes the room the rows leave.
    if let (Some(n), Some(((_, lx, baseline, anchor), row, cy))) = (&note, note_at) {
        let marks = c.mark_boxes();
        let own = (
            cy - step / 2.0 + 0.75 * vs.size,
            cy + step / 2.0 - 0.2 * vs.size,
        );
        if !c.stand_in(n, (lx, anchor, baseline), own, &marks) {
            let gap = c.t.marks.label_padding;
            let spots = |c: &Ctx, w: f32| {
                c.note_spots(w, row, gap, (left, right))
                    .into_iter()
                    .map(|(x, anchor)| (x, anchor, baseline))
                    .collect()
            };
            let rows = (top + 0.75 * ls.size, top + span - 0.2 * ls.size);
            c.note(n, spots, rows, &marks, (row, left - dot));
        }
    }
    let bottom = top + span;
    let tick_labels: Vec<(String, f32)> = ticks(lo, hi, DUMBBELL_TICKS)
        .into_iter()
        .map(|v| (tf.format(v), x.map(v)))
        .collect();
    let below = c.bottom_labels(&tick_labels, bottom, Under::Values);
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    bottom + below + PAD - c.top
}

/// The dumbbell's dot, as a Vega symbol size, as the gallery sets it.
const DUMBBELL_DOT: f32 = 90.0;

/// The dumbbell's plot height, as the gallery's dumbbell sets it.
const DUMBBELL_HEIGHT: f32 = 190.0;

/// Ticks on the dumbbell's value axis, as the gallery's dumbbell asks for.
const DUMBBELL_TICKS: f64 = 7.0;

/// How far a dumbbell's note stands from its dot's centre, as the gallery's does.
const DUMBBELL_NOTE_DX: f32 = 9.0;
