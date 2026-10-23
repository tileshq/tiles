//! Ranked horizontal bars, and the lollipop and dot forms drawn the same way.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear, nice, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Space between a lollipop's average rule and text beside it.
const RULE_GAP: f32 = 4.0;

/// Ticks on a lollipop's value axis, as the gallery's lollipop asks for.
const LOLLIPOP_TICKS: f64 = 5.0;

/// A lollipop's plot height and top padding, as the gallery's lollipop sets them.
const LOLLIPOP_HEIGHT: f32 = 240.0;

const LOLLIPOP_TOP: f32 = 18.0;

/// A dashed rule's dash and gap.
const DASH: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BarStyle {
    Bar,
    Lollipop,
    Dot,
}

/// Ranked horizontal bars, stems or dots, labels on the left, values at
/// the end.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn bars(c: &mut Ctx, style: BarStyle) -> f32 {
    let pts = c.categories("y");
    let values: Vec<f64> = pts.iter().map(|p| p.value).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let vs = c.row_value_style();
    let off = c.t.marks.row_value_offset;
    let names: Vec<String> = pts.iter().map(|p| p.label.clone()).collect();
    let labels = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = labels.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    // The first named row carries the note, after its value or wherever the
    // bars leave room.
    let noted = pts.iter().position(|p| c.is_highlight(&p.label));
    let note = c.intent.note.clone().filter(|_| noted.is_some());
    let dot = c.t.marks.head_radius;
    let extra = if style == BarStyle::Bar { 0.0 } else { dot };
    let left = c.left + PAD + widest + c.t.marks.label_padding + extra;
    let lollipop = style == BarStyle::Lollipop;
    // A lollipop marks the average with a dashed rule, named above the plot,
    // over a value axis, as the gallery's lollipop does. One row is its own
    // mean, so it has none unless the intent sends an average.
    let average = if lollipop {
        c.col("average")
            .and_then(|a| c.intent.table.rows.iter().find_map(|r| r[a].number()))
            .or_else(|| {
                (pts.len() > 1).then(|| values.iter().sum::<f64>() / f64::from(f(values.len())))
            })
    } else {
        None
    };
    let (lo, hi) = if style == BarStyle::Dot {
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        nice(min, max, 10.0)
    } else {
        let scaled = values.iter().copied().chain(average);
        nice(
            scaled.clone().fold(0.0, f64::min),
            scaled.fold(0.0, f64::max),
            if lollipop { LOLLIPOP_TICKS } else { 10.0 },
        )
    };
    let tf = c.tick_format(&ticks(lo, hi, LOLLIPOP_TICKS), true);
    // The scale runs to the card's edge and only narrows as far as the
    // value label that would overhang it needs, as Vega's fit-x does: with a
    // nice domain the longest bar rarely reaches the end.
    let limit = c.left + c.width - PAD;
    let label_w = |v: f64| c.width_of(&nf.format(v), vs);
    let widest_value = values.iter().map(|v| label_w(*v)).fold(0.0, f32::max);
    let value_room = widest_value + off;
    // A value is written past its bar's far end from zero: a negative bar's
    // right of zero.
    let zero = lo.max(0.0).min(hi);
    let right = values.iter().fold(limit, |right, v| {
        #[allow(clippy::cast_possible_truncation)] // A share of the scale, 0 to 1.
        let t = ((v.max(zero) - lo) / (hi - lo)) as f32;
        let room = limit - left - extra - off - label_w(*v);
        if t > 0.0 {
            right.min(left + room / t)
        } else {
            right
        }
    });
    let mut right = right.max(limit - value_room - extra).max(left + 1.0);
    if lollipop {
        right = right.min(limit - c.width_of(&tf.format(hi), ls) / 2.0);
    }
    // A value written past the average's rule fits on the card too.
    if let Some(avg) = average {
        #[allow(clippy::cast_possible_truncation)] // A share of the scale, 0 to 1.
        let t = ((avg - lo) / (hi - lo)) as f32;
        if t > 0.0 {
            right = right.min(left + (limit - left - RULE_GAP - widest_value) / t);
        }
    }
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let inner = c.t.marks.band_padding_inner;
    let outer = c.t.marks.band_padding_outer;
    // Few rows fill the plot height, as the prototype's ranked bar does;
    // many rows keep a readable step and the card grows.
    let span_of = |step: f32| step * (f(pts.len()) - inner + 2.0 * outer);
    let height = if lollipop {
        LOLLIPOP_HEIGHT
    } else {
        c.t.marks.plot_height
    };
    let step = c.t.marks.row_step.max(height / span_of(1.0));
    // Room above the rows for the average's name: the gallery's top padding
    // and the name's line over it.
    let named_room = if lollipop {
        LOLLIPOP_TOP - PAD + average.map_or(0.0, |_| (ls.size + 2.0).ceil())
    } else {
        0.0
    };
    let top = c.top + PAD + named_room;
    let span = span_of(step);
    let band = Band::new(pts.len(), top, top + span, inner, outer);
    let base = x.map(zero);
    let dashes_from = c.items.len();
    if let Some(avg) = average {
        let ax = x.map(avg);
        let shade = (Emphasis::Neutral(0), c.pal.neutrals[0]);
        let mut y0 = top;
        while y0 < top + span {
            let dash = Shape::Line {
                points: vec![(ax, y0), (ax, (y0 + DASH).min(top + span))],
                width: 1.0,
            };
            c.items
                .push(Item::new(Part::Guide, dash, shade.0, shade.1).grow(Grow::Fade));
            y0 += 2.0 * DASH;
        }
        let text = format!("Average {}", nf.format(avg));
        let w = c.width_of(&text, ls);
        let (tx, anchor) = if ax + RULE_GAP + w <= limit {
            (ax + RULE_GAP, Anchor::Start)
        } else {
            (ax - RULE_GAP, Anchor::End)
        };
        let at = c.items.len();
        c.text(
            Part::AxisLabel,
            text,
            tx,
            top - 2.0 - (0.2 * ls.size).round(),
            ls,
            anchor,
            shade,
        );
        c.items[at].grow = Grow::Fade;
    }
    let dashes = dashes_from..c.items.len();
    let mut note_at = None;
    for (i, p) in pts.iter().enumerate() {
        let hl = c.is_highlight(&p.label);
        let (e, col) = c.mark_color(hl);
        let cy = band.center(i);
        let xv = x.map(p.value);
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::AxisLabel,
            labels[i].clone(),
            c.left + PAD + widest,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            muted,
        );
        let tip = c.tip(&p.rows, &nf);
        match style {
            BarStyle::Bar => {
                let thick = band.width * c.t.marks.bar_band;
                let (y0, y1) = (cy - thick / 2.0, cy + thick / 2.0);
                let (rect, corners) = if p.value >= 0.0 {
                    (Rect::from_ltrb(base, y0, xv, y1), Corners::Right)
                } else {
                    (Rect::from_ltrb(xv, y0, base, y1), Corners::Left)
                };
                let shape = Shape::Rect {
                    rect,
                    radius: c.t.marks.bar_radius,
                    corners,
                };
                c.items.push(
                    Item::new(Part::Mark, shape, e, col)
                        .datum(p.rows[0], tip)
                        .highlighted(hl)
                        .grow(Grow::FromX(base)),
                );
            }
            BarStyle::Lollipop => {
                let stem = Shape::Line {
                    points: vec![(base, cy), (xv, cy)],
                    width: c.t.marks.stem_width,
                };
                c.items.push(
                    Item::new(Part::Mark, stem, e, col)
                        .datum(p.rows[0], tip.clone())
                        .highlighted(hl)
                        .grow(Grow::FromX(base)),
                );
                // An unnamed head is a shade darker than its stem, as in the gallery.
                let (he, hc) = if e == Emphasis::Grey {
                    (Emphasis::Neutral(0), c.pal.neutrals[0])
                } else {
                    (e, col)
                };
                let head = Shape::Circle { cx: xv, cy, r: dot };
                c.items.push(
                    Item::new(Part::Mark, head, he, hc)
                        .datum(p.rows[0], tip)
                        .highlighted(hl)
                        .grow(Grow::FromX(base)),
                );
            }
            BarStyle::Dot => {
                let track = Shape::Line {
                    points: vec![(left, cy), (right, cy)],
                    width: 1.0,
                };
                c.items
                    .push(Item::new(Part::Guide, track, Emphasis::Grid, c.pal.grid));
                let head = Shape::Circle { cx: xv, cy, r: dot };
                c.items.push(
                    Item::new(Part::Mark, head, e, col)
                        .datum(p.rows[0], tip)
                        .highlighted(hl)
                        .grow(Grow::FromX(left)),
                );
            }
        }
        let end = if style == BarStyle::Bar { xv } else { xv + dot };
        let vc = c.row_value_color(hl);
        let at = c.items.len();
        let text = nf.format(p.value);
        let w = c.width_of(&text, vs);
        let mut lx = end.max(base) + off;
        // A value that would cross the average's rule is written past it.
        if let Some(ax) = average.map(|a| x.map(a))
            && ax + RULE_GAP > lx
            && ax - RULE_GAP < lx + w
        {
            lx = ax + RULE_GAP;
        }
        let after = lx + w + c.t.marks.label_padding;
        c.text(
            Part::Value,
            text,
            lx,
            cy + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Start,
            vc,
        );
        c.items[at].grow = Grow::Fade;
        if Some(i) == noted {
            let mark = if style == BarStyle::Dot {
                Rect::from_ltrb(xv - dot, cy - dot, xv + dot, cy + dot)
            } else {
                Rect::from_ltrb(base.min(end), cy - dot, base.max(end), cy + dot)
            };
            let mut own = mark;
            if let Some(b) = c.text_box(&c.items[at]) {
                own.join(b);
            }
            note_at = Some((after, cy + (MIDDLE * ls.size).round(), mark, own));
        }
    }
    // After the named row's value, else wherever the bars leave room.
    if let (Some(n), Some((nx, ny, mark, own))) = (&note, note_at) {
        // A note may cross the average's rule on its knockout, as it may a
        // gridline.
        let marks: Vec<Rect> = c
            .items
            .iter()
            .enumerate()
            .filter(|(k, i)| matches!(i.part, Part::Mark | Part::Guide) && !dashes.contains(k))
            .map(|(_, i)| i.bounds())
            .collect();
        let rows = (top + 0.75 * ls.size, top + span - 0.2 * ls.size);
        let gap = c.t.marks.label_padding;
        let spots = |c: &Ctx, w: f32| {
            std::iter::once((nx, Anchor::Start))
                .chain(c.note_spots(w, mark, gap, (left, limit)))
                .map(|(x, anchor)| (x, anchor, ny))
                .collect()
        };
        c.note(n, spots, rows, &marks, (own, left));
    }
    c.plot = Rect::from_ltrb(left, top, right, top + span);
    let below = if lollipop {
        let tick_labels: Vec<(String, f32)> = ticks(lo, hi, LOLLIPOP_TICKS)
            .into_iter()
            .map(|v| (tf.format(v), x.map(v)))
            .collect();
        c.bottom_labels(&tick_labels, top + span, Under::Values)
    } else {
        0.0
    };
    named_room + span + below + 2.0 * PAD
}
