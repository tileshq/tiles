//! The box plot: each group's spread from its own values.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f, symbol_radius};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, ticks};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The box plot's plot height, as the gallery's box plot sets it.
const HEIGHT: f32 = 170.0;

/// Most room one group takes, so a few groups stay close together.
const MAX_STEP: f32 = 60.0;

/// A box's height, as the gallery sets its size.
const BOX: f32 = 22.0;

/// How far past the box a whisker may reach, in boxes (Tukey's 1.5).
const WHISKER: f64 = 1.5;

/// An outlier's dot, as a Vega symbol size, and its opacity.
const OUTLIER: f32 = 18.0;
const OUTLIER_OPACITY: f32 = 0.7;

/// The median's stroke across the box.
const MEDIAN_WIDTH: f32 = 2.0;

/// The narrowest box drawn, for a group with no spread.
const MIN_BOX: f32 = 4.0;

/// Ticks on the value axis, as the gallery's box plot asks for.
const TICKS: f64 = 6.0;

/// One group's summary.
struct Summary {
    label: String,
    rows: Vec<usize>,
    q1: f64,
    median: f64,
    q3: f64,
    low: f64,
    high: f64,
    /// Values past the whiskers, with their rows.
    outliers: Vec<(usize, f64)>,
}

/// The `p` quantile of sorted values, interpolated between the two nearest
/// ranks as d3 and Vega compute it.
pub(super) fn quantile(sorted: &[f64], p: f64) -> f64 {
    match sorted.len() {
        0 => 0.0,
        1 => sorted[0],
        n => {
            let at = p.clamp(0.0, 1.0) * f64::from(f(n - 1));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // An index below n.
            let i = (at.floor() as usize).min(n - 2);
            let t = at - f64::from(f(i));
            sorted[i] + (sorted[i + 1] - sorted[i]) * t
        }
    }
}

fn summaries(c: &Ctx) -> Vec<Summary> {
    let (xc, yc) = (c.col("x").unwrap_or(0), c.col("y").unwrap_or(0));
    let mut groups: Vec<(String, Vec<(usize, f64)>)> = Vec::new();
    for (r, row) in c.intent.table.rows.iter().enumerate() {
        let Some(value) = row[yc].number() else {
            continue;
        };
        let label = row[xc].label();
        match groups.iter_mut().find(|g| g.0 == label) {
            Some(g) => g.1.push((r, value)),
            None => groups.push((label, vec![(r, value)])),
        }
    }
    groups
        .into_iter()
        .map(|(label, mut values)| {
            values.sort_by(|a, b| a.1.total_cmp(&b.1));
            let sorted: Vec<f64> = values.iter().map(|v| v.1).collect();
            let (q1, median, q3) = (
                quantile(&sorted, 0.25),
                quantile(&sorted, 0.5),
                quantile(&sorted, 0.75),
            );
            let reach = WHISKER * (q3 - q1);
            let inside = |v: f64| v >= q1 - reach && v <= q3 + reach;
            let low = sorted.iter().copied().find(|v| inside(*v)).unwrap_or(q1);
            let high = sorted
                .iter()
                .rev()
                .copied()
                .find(|v| inside(*v))
                .unwrap_or(q3);
            Summary {
                label,
                rows: values.iter().map(|v| v.0).collect(),
                q1,
                median,
                q3,
                low,
                high,
                outliers: values.into_iter().filter(|v| !inside(v.1)).collect(),
            }
        })
        .collect()
}

/// A box from the 25th to the 75th percentile per group with its median
/// across it, whiskers to the furthest values within 1.5 boxes and dots
/// past them, over a value axis; the named group takes the accent (red for
/// a bad finding) and the rest are grey.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn boxes(c: &mut Ctx) -> f32 {
    let groups = summaries(c);
    let ls = c.label_style();
    let names: Vec<String> = groups.iter().map(|g| g.label.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    let all: Vec<f64> = groups
        .iter()
        .flat_map(|g| {
            [g.low, g.high]
                .into_iter()
                .chain(g.outliers.iter().map(|o| o.1))
        })
        .collect();
    let min = all.iter().copied().fold(f64::INFINITY, f64::min);
    let max = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // Times and counts read from zero, as the gallery's days to close do,
    // unless the values sit far from it.
    let from = if min >= 0.0 && min <= max / 2.0 {
        0.0
    } else {
        min
    };
    let (lo, hi) = nice(from, max, TICKS);
    let value_ticks = ticks(lo, hi, TICKS);
    let tf = c.tick_format(&value_ticks, true);
    let nf = NumberFormat::for_values(&all).with_unit(c.intent.unit);
    let left = c.left + PAD + widest + c.t.marks.label_padding;
    let right = (c.left + c.width - PAD - c.width_of(&tf.format(hi), ls) / 2.0).max(left + 1.0);
    let x = Linear {
        d0: lo,
        d1: hi,
        r0: left,
        r1: right,
    };
    let step = (c.t.marks.row_step + 8.0).max((HEIGHT / f(groups.len().max(1))).min(MAX_STEP));
    let top = c.top + PAD;
    let span = step * f(groups.len());
    let x_name = c
        .col("x")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let y_name = c
        .col("y")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let whisker = (Emphasis::Muted, c.pal.muted);
    let dot = symbol_radius(OUTLIER);
    for (i, g) in groups.iter().enumerate() {
        let cy = top + step * (f(i) + 0.5);
        let hl = c.is_highlight(&g.label);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            c.left + PAD + widest,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            (Emphasis::Muted, c.pal.muted),
        );
        for (a, b) in [(g.low, g.q1), (g.q3, g.high)] {
            if (b - a).abs() > 0.0 {
                let shape = Shape::Line {
                    points: vec![(x.map(a), cy), (x.map(b), cy)],
                    width: 1.0,
                };
                c.items
                    .push(Item::new(Part::Guide, shape, whisker.0, whisker.1).grow(Grow::Fade));
            }
        }
        let tip = format!(
            "{x_name}: {}\nmedian {y_name}: {}\nmiddle half: {} to {}\nwhiskers: {} to {}\nvalues: {}",
            g.label,
            nf.format(g.median),
            nf.format(g.q1),
            nf.format(g.q3),
            nf.format(g.low),
            nf.format(g.high),
            g.rows.len()
        );
        let (e, col) = c.mark_color(hl);
        // A group with no spread, such as one value, still draws a box,
        // narrow and with no median line to hide it.
        let (x0, x1) = (x.map(g.q1), x.map(g.q3));
        let narrow = x1 - x0 < MIN_BOX;
        let (x0, x1) = if narrow {
            let mid = x.map(g.median);
            (mid - MIN_BOX / 2.0, mid + MIN_BOX / 2.0)
        } else {
            (x0, x1)
        };
        let rect = Rect::from_ltrb(x0, cy - BOX / 2.0, x1, cy + BOX / 2.0);
        let shape = Shape::Rect {
            rect,
            radius: c.t.marks.bar_radius,
            corners: Corners::All,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(g.rows[0], tip)
                .highlighted(hl)
                .grow(Grow::Fade),
        );
        if !narrow {
            let mx = x.map(g.median);
            let shape = Shape::Line {
                points: vec![(mx, cy - BOX / 2.0), (mx, cy + BOX / 2.0)],
                width: MEDIAN_WIDTH,
            };
            c.items
                .push(Item::new(Part::Guide, shape, Emphasis::Card, c.pal.card).grow(Grow::Fade));
        }
        // Outliers stay muted in every group, as the gallery draws them: the
        // box carries the name.
        for &(r, v) in &g.outliers {
            let shape = Shape::Circle {
                cx: x.map(v),
                cy,
                r: dot,
            };
            let tip = format!("{x_name}: {}\n{y_name}: {}", g.label, nf.format(v));
            c.items.push(
                Item::new(
                    Part::Mark,
                    shape,
                    whisker.0,
                    whisker.1.with_opacity(OUTLIER_OPACITY),
                )
                .datum(r, tip)
                .grow(Grow::Fade),
            );
        }
    }
    let bottom = top + span;
    let tick_labels: Vec<(String, f32)> = value_ticks
        .iter()
        .map(|v| (tf.format(*v), x.map(*v)))
        .collect();
    let below = c.bottom_labels(&tick_labels, bottom, Under::Values);
    let titled = c.axis_title((left, right), bottom + below);
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    bottom + below + titled + PAD - c.top
}

#[cfg(test)]
mod tests {
    use super::quantile;

    #[test]
    fn quantiles_interpolate_between_ranks_as_d3_does() {
        let v = [1.0, 2.0, 3.0, 4.0];
        assert!((quantile(&v, 0.25) - 1.75).abs() < 1e-12);
        assert!((quantile(&v, 0.5) - 2.5).abs() < 1e-12);
        assert!((quantile(&v, 0.75) - 3.25).abs() < 1e-12);
        assert!((quantile(&v, 1.0) - 4.0).abs() < 1e-12);
        assert!((quantile(&[7.0], 0.25) - 7.0).abs() < 1e-12);
        assert!(quantile(&[], 0.5).abs() < 1e-12);
    }
}
