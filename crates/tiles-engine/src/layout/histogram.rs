//! The histogram: touching columns that count values into bins.

use skia_safe::Rect;
use tiles_intent::{Intent, IntentError};

use super::{Ctx, PAD, TOP, Under, f};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, tick_step, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Most bins a histogram draws, sent or counted.
pub(crate) const MAX_BINS: usize = 60;

/// The histogram's plot height, as the gallery's histogram sets it.
const HEIGHT: f32 = 200.0;

/// Ticks on the count axis, as the gallery's histogram asks for.
const COUNT_TICKS: f32 = 4.0;

/// Ticks on the value axis.
const VALUE_TICKS: f64 = 6.0;

/// The gap between two bins, so each reads as its own column.
const BIN_GAP: f32 = 1.0;

/// A dashed rule's dash and gap.
const DASH: f32 = 3.0;

/// Space between the threshold rule and its label.
const RULE_GAP: f32 = 4.0;

/// Fewest and most bins Tiles counts raw values into.
const AUTO_BINS: (f32, f32) = (5.0, 30.0);

/// One bin: where it starts and ends, its count and the rows in it.
struct Bin {
    start: f64,
    end: f64,
    count: f64,
    rows: Vec<usize>,
}

/// Refuses counts below zero and more bins than the form draws.
pub(super) fn check(intent: &Intent) -> Result<(), IntentError> {
    let Some(x) = intent.column("x") else {
        return Ok(());
    };
    let Some(y) = intent.column("y") else {
        return Ok(());
    };
    let mut starts: Vec<f64> = Vec::new();
    for row in &intent.table.rows {
        let (Some(s), Some(n)) = (row[x].number(), row[y].number()) else {
            continue;
        };
        if n < 0.0 {
            return Err(IntentError::new(
                "data.rows",
                "histogram counts must be zero or more",
                "send how many values fall in each bin, or leave y out and send the values",
            ));
        }
        if !starts.contains(&s) {
            starts.push(s);
        }
    }
    if starts.is_empty() {
        return Err(IntentError::new(
            "data.rows",
            "no row holds both an x and a y number",
            "fill both columns in at least one row, or leave y out and send the values",
        ));
    }
    if starts.len() > MAX_BINS {
        return Err(IntentError::new(
            "data.rows",
            format!(
                "{} bins is more than form histogram draws ({MAX_BINS})",
                starts.len()
            ),
            "send wider bins, or leave y out and let Tiles bin the values",
        ));
    }
    // A threshold inside a sent bin splits a count Tiles cannot split, so
    // the share past it would be wrong.
    if let Some(t) = threshold(intent) {
        let bins = bins(intent, None);
        let inside = bins
            .iter()
            .any(|b| t > b.start + edge(b) && t < b.end - edge(b));
        if inside {
            return Err(IntentError::new(
                "fields.threshold",
                "the threshold falls inside a bin, so the share past it cannot be counted",
                "send a threshold that is one of the bins' starts, or leave y out and send the values",
            ));
        }
    }
    Ok(())
}

/// How near a bin's edge a threshold counts as on it.
fn edge(b: &Bin) -> f64 {
    (b.end - b.start).abs() * 1e-9
}

/// The threshold the intent sends: the first row's number.
fn threshold(intent: &Intent) -> Option<f64> {
    let t = intent.column("threshold")?;
    intent.table.rows.iter().find_map(|r| r[t].number())
}

/// The bins: the sent ones with their counts summed per start, each as wide
/// as the narrowest gap between two starts, or the values counted into
/// even, round bins, one of whose edges is the threshold when it falls
/// among the values, so the bins past it hold exactly the values past it.
fn bins(intent: &Intent, threshold: Option<f64>) -> Vec<Bin> {
    let x = intent.column("x").unwrap_or(0);
    let rows = &intent.table.rows;
    if let Some(y) = intent.column("y") {
        let mut sent: Vec<Bin> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let (Some(s), Some(n)) = (row[x].number(), row[y].number()) else {
                continue;
            };
            match sent.iter_mut().find(|b| b.start.total_cmp(&s).is_eq()) {
                Some(b) => {
                    b.count += n;
                    b.rows.push(r);
                }
                None => sent.push(Bin {
                    start: s,
                    end: s,
                    count: n,
                    rows: vec![r],
                }),
            }
        }
        sent.sort_by(|a, b| a.start.total_cmp(&b.start));
        let width = sent
            .windows(2)
            .map(|w| w[1].start - w[0].start)
            .filter(|g| *g > 0.0)
            .fold(f64::INFINITY, f64::min);
        let width = if width.is_finite() { width } else { 1.0 };
        for b in &mut sent {
            b.end = b.start + width;
        }
        return sent;
    }
    let values: Vec<(usize, f64)> = rows
        .iter()
        .enumerate()
        .filter_map(|(r, row)| row[x].number().map(|v| (r, v)))
        .collect();
    let min = values.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
    let max = values.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max);
    if !(min.is_finite() && max.is_finite()) {
        return Vec::new();
    }
    // Sturges' count, between the form's bounds, on a round step.
    let target = (f(values.len()).log2().ceil() + 1.0).clamp(AUTO_BINS.0, AUTO_BINS.1);
    let (lo, hi) = if max > min {
        (min, max)
    } else {
        nice(min, max, f64::from(target))
    };
    let step = tick_step(lo, hi, f64::from(target));
    let first = match threshold.filter(|t| *t <= hi) {
        Some(t) => t - ((t - lo) / step).ceil() * step,
        None => (lo / step).floor() * step,
    };
    let mut out: Vec<Bin> = Vec::new();
    let mut start = first;
    while start <= hi && out.len() < MAX_BINS {
        out.push(Bin {
            start,
            end: start + step,
            count: 0.0,
            rows: Vec::new(),
        });
        start = first + step * f64::from(f(out.len()));
    }
    if out.is_empty() {
        return out;
    }
    let last = out.len() - 1;
    for (r, v) in values {
        // A value on the last edge counts in the last bin.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A bin index within MAX_BINS.
        let i = (((v - first) / step + 1e-9).floor().max(0.0) as usize).min(last);
        out[i].count += 1.0;
        out[i].rows.push(r);
    }
    out
}

/// Touching columns that count each bin over a value axis, a count axis
/// on the left, and an optional threshold drawn as a dashed rule with the
/// bins from it on in the accent (red for a bad finding).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn histogram(c: &mut Ctx) -> f32 {
    let threshold = threshold(c.intent);
    let bins = bins(c.intent, threshold);
    let ls = c.label_style();
    let vs = c.value_style();
    let (Some(first), Some(last)) = (bins.first(), bins.last()) else {
        return 0.0;
    };
    let mut lo = first.start;
    let mut hi = last.end;
    if let Some(t) = threshold {
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let counts: Vec<f64> = bins.iter().map(|b| b.count).collect();
    let (_, top_count) = nice(0.0, counts.iter().copied().fold(0.0, f64::max), 4.0);
    // Whole counts take whole ticks: one value reads 0 and 1, not 0.2 steps.
    #[allow(clippy::cast_possible_truncation)] // A small count of ticks.
    let count_ticks = if counts.iter().all(|n| n.fract() == 0.0) {
        COUNT_TICKS.min(top_count.max(1.0) as f32)
    } else {
        COUNT_TICKS
    };
    let edges: Vec<f64> = bins.iter().flat_map(|b| [b.start, b.end]).collect();
    let xf = NumberFormat::for_values(&edges).with_unit(c.intent.unit);
    let cf = NumberFormat::for_values(&counts);
    let left = c.left
        + PAD
        + c.tick_label_width_with(0.0, top_count, count_ticks, false)
        + c.t.marks.label_padding;
    let value_ticks = ticks(lo, hi, VALUE_TICKS);
    let tf = NumberFormat::for_values(&value_ticks).with_unit(c.intent.unit);
    let right = c.left + c.width - PAD - c.width_of(&tf.format(hi), ls) / 2.0;
    let right = right.max(left + 1.0);
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    // A row above the bins for the threshold's label.
    let label_room = if threshold.is_some() {
        (vs.size + 6.0).ceil()
    } else {
        (ls.size * 0.5).ceil()
    };
    let top = c.top + PAD + label_room;
    let y = Linear {
        d0: 0.0,
        d1: top_count,
        r0: top + HEIGHT,
        r1: top,
    };
    c.y_axis_with(&y, Some(count_ticks), (left, right), false);
    let base = y.map(0.0);
    let x_name = c
        .col("x")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let count_name = c.col("y").map_or_else(
        || "count".to_string(),
        |i| c.intent.table.columns[i].clone(),
    );
    let named = |b: &Bin| threshold.is_some_and(|t| b.start >= t - edge(b));
    let mut share = (0.0, 0.0);
    for b in &bins {
        share.1 += b.count;
        if named(b) {
            share.0 += b.count;
        }
        let Some(&row) = b.rows.first() else {
            continue;
        };
        if b.count <= 0.0 {
            continue;
        }
        let hl = named(b);
        let (e, col) = if threshold.is_none() {
            (Emphasis::Accent, c.pal.accent)
        } else if hl {
            c.named()
        } else {
            (Emphasis::Grey, c.pal.grey)
        };
        let (x0, x1) = (x.map(b.start), x.map(b.end));
        let rect = Rect::from_ltrb(
            x0 + BIN_GAP / 2.0,
            y.map(b.count),
            (x1 - BIN_GAP / 2.0).max(x0 + BIN_GAP / 2.0 + 0.5),
            base,
        );
        let tip = format!(
            "{x_name}: {} to {}\n{count_name}: {}",
            xf.format(b.start),
            xf.format(b.end),
            cf.format(b.count)
        );
        let shape = Shape::Rect {
            rect,
            radius: c.t.marks.bar_radius,
            corners: Corners::Top,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(row, tip)
                .highlighted(hl)
                .grow(Grow::FromY(base)),
        );
    }
    if let Some(t) = threshold {
        let tx = x.map(t);
        let ink = (Emphasis::Ink, c.pal.ink);
        let rule_top = top - label_room + 2.0;
        let mut y0 = rule_top;
        while y0 < base {
            let dash = Shape::Line {
                points: vec![(tx, y0), (tx, (y0 + DASH).min(base))],
                width: 1.0,
            };
            c.items
                .push(Item::new(Part::Guide, dash, ink.0, ink.1).grow(Grow::Fade));
            y0 += 2.0 * DASH;
        }
        #[allow(clippy::cast_possible_truncation)] // A percentage, 0 to 100.
        let pct = if share.1 > 0.0 {
            (share.0 / share.1 * 100.0).round()
        } else {
            0.0
        };
        let text = format!(
            "{}% at {} or more",
            NumberFormat::for_values(&[pct]).format(pct),
            NumberFormat::for_values(&[t])
                .with_unit(c.intent.unit)
                .format(t)
        );
        let w = c.width_of(&text, vs);
        let (lx, anchor) = if tx + RULE_GAP + w <= c.left + c.width - PAD {
            (tx + RULE_GAP, Anchor::Start)
        } else {
            (tx - RULE_GAP, Anchor::End)
        };
        let at = c.items.len();
        let color = c.named();
        c.text(
            Part::Value,
            text,
            lx,
            rule_top + (TOP * vs.size).round(),
            vs,
            anchor,
            color,
        );
        c.items[at].grow = Grow::Fade;
    }
    let tick_labels: Vec<(String, f32)> = value_ticks
        .iter()
        .map(|v| (tf.format(*v), x.map(*v)))
        .collect();
    let below = c.bottom_labels(&tick_labels, base, Under::Values);
    let titled = c.axis_title((left, right), base + below);
    c.plot = Rect::from_ltrb(left, top, right, base);
    base + below + titled + PAD - c.top
}
