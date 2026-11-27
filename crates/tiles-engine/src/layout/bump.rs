//! The bump chart: ranks across periods.

use skia_safe::Rect;

use super::{Cells, Ctx, LABEL_SHARE, MIDDLE, PAD, Under, cells, f, series_colors, spread};
use crate::format::NumberFormat;
use crate::scale::points;
use crate::scene::{Anchor, Emphasis, Grow, Item, Part, Shape};

/// The plot height, as the gallery's bump chart sets it.
const HEIGHT: f32 = 230.0;

/// Fewest pixels between two ranks.
const RANK_STEP: f32 = 22.0;

/// Space between a line's last point and its name.
const END_LABEL_GAP: f32 = 8.0;

/// Each series' rank in each period, 1 for the largest value, ties kept in
/// series order; `None` where the series has no value.
fn ranks(grid: &Cells, periods: usize, series: usize) -> Vec<Vec<Option<usize>>> {
    let mut out = vec![vec![None; periods]; series];
    for xi in 0..periods {
        let mut present: Vec<(usize, f64)> = (0..series)
            .filter_map(|si| grid[xi][si].as_ref().map(|(v, _)| (si, *v)))
            .collect();
        present.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        for (rank, (si, _)) in present.into_iter().enumerate() {
            out[si][xi] = Some(rank + 1);
        }
    }
    out
}

/// One line per series with a number through its rank in each period, first
/// at the top, ranks named down the left as far as the most series any one
/// period ranks, and each series named at its last point. The named series
/// takes the accent (red for a bad finding) and the rest the ordered context
/// greys (playbook K1); with none named, the categorical order (K6).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn bump(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let series: Vec<String> = match (c.col("series"), c.col("y")) {
        (Some(sc), Some(y)) => {
            let mut out: Vec<String> = Vec::new();
            for row in c
                .intent
                .table
                .rows
                .iter()
                .filter(|r| r[y].number().is_some())
            {
                let l = row[sc].label();
                if !out.contains(&l) {
                    out.push(l);
                }
            }
            out
        }
        _ => Vec::new(),
    };
    let grid = cells(c, &xs, &series);
    let rank = ranks(&grid, xs.len(), series.len());
    let values: Vec<f64> = grid.iter().flatten().flatten().map(|(v, _)| *v).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let n = rank.iter().flatten().flatten().copied().max().unwrap_or(1);
    let (colors, hl) = series_colors(c, &series, false);
    let spotlight = hl.iter().any(|h| *h);
    let rank_labels: Vec<String> = (1..=n).map(|r| format!("#{r}")).collect();
    let rank_w = rank_labels
        .iter()
        .map(|l| c.width_of(l, ls))
        .fold(0.0, f32::max);
    let names = c.fit_all(&series, ls.with_weight(600), LABEL_SHARE / 2.0);
    let name_room = names
        .iter()
        .map(|l| c.width_of(l, ls.with_weight(600)))
        .fold(0.0, f32::max)
        + END_LABEL_GAP;
    let left = c.left + PAD + rank_w + c.t.marks.label_padding;
    let right = (c.left + c.width - PAD - name_room).max(left + 1.0);
    let top = c.top + PAD + (ls.size * 0.5).ceil();
    let h = HEIGHT
        .max(RANK_STEP * f(n))
        .max((ls.size + 2.0) * f(series.len()));
    let ys = points(n, top, top + h, 0.5);
    let xpos = points(xs.len(), left, right, 0.3);
    let muted = (Emphasis::Muted, c.pal.muted);
    for (r, label) in rank_labels.iter().enumerate() {
        c.text(
            Part::AxisLabel,
            label.clone(),
            left - c.t.marks.label_padding,
            ys[r] + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            muted,
        );
    }
    let mut order: Vec<usize> = (0..series.len()).collect();
    order.sort_by_key(|&i| hl[i]);
    let x_name = c
        .col("x")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let s_name = c
        .col("series")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let y_name = c
        .col("y")
        .map(|i| c.intent.table.columns[i].clone())
        .unwrap_or_default();
    let mut ends: Vec<(usize, f32)> = Vec::new();
    for &si in &order {
        let (e, col) = colors[si];
        // A spotlight draws the named line firm and the rest thin with
        // small points, as a named multi-line does (playbook M4, M8).
        let (width, point) = match (spotlight, hl[si]) {
            (true, true) => (c.t.marks.line_width_named, c.t.marks.point_radius_highlight),
            (true, false) => (c.t.marks.line_width_rest, c.t.marks.point_radius),
            _ => (c.t.marks.line_width, c.t.marks.point_radius_highlight),
        };
        // A gap in a series breaks its line rather than bridging it.
        let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut run = Vec::new();
        for xi in 0..xs.len() {
            match rank[si][xi] {
                Some(r) => run.push((xpos[xi], ys[r - 1])),
                None if !run.is_empty() => runs.push(std::mem::take(&mut run)),
                None => {}
            }
        }
        runs.push(run);
        let first_row = grid.iter().find_map(|cells| {
            cells[si]
                .as_ref()
                .and_then(|(_, rows)| rows.first().copied())
        });
        for run in runs.into_iter().filter(|r| r.len() > 1) {
            let mut item = Item::new(Part::Mark, Shape::Line { points: run, width }, e, col)
                .highlighted(hl[si])
                .grow(Grow::Wipe);
            item.datum = first_row;
            c.items.push(item);
        }
        for xi in 0..xs.len() {
            let (Some(r), Some((v, rows))) = (rank[si][xi], grid[xi][si].as_ref()) else {
                continue;
            };
            let tip = format!(
                "{s_name}: {}\n{x_name}: {}\nrank: #{r}\n{y_name}: {}",
                series[si],
                xs[xi],
                nf.format(*v)
            );
            let shape = Shape::Circle {
                cx: xpos[xi],
                cy: ys[r - 1],
                r: point,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(rows[0], tip)
                    .highlighted(hl[si])
                    .grow(Grow::Wipe),
            );
        }
        if let Some(r) = (0..xs.len()).rev().find_map(|xi| rank[si][xi]) {
            ends.push((si, ys[r - 1]));
        }
    }
    spread(&mut ends, ls.size + 2.0, (top, top + h));
    for &(si, yy) in &ends {
        let last = (0..xs.len())
            .rev()
            .find(|&xi| rank[si][xi].is_some())
            .unwrap_or(0);
        let (style, color) = if hl[si] {
            (ls.with_weight(600), colors[si])
        } else if spotlight {
            (ls, muted)
        } else {
            (ls, colors[si])
        };
        let at = c.items.len();
        c.text(
            Part::Legend,
            names[si].clone(),
            xpos[last] + END_LABEL_GAP,
            yy + (MIDDLE * style.size).round(),
            style,
            Anchor::Start,
            color,
        );
        c.items[at].grow = Grow::Fade;
    }
    let bottom = top + h;
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    let labels: Vec<(String, f32)> = xs.iter().cloned().zip(xpos).collect();
    let below = c.bottom_labels(&labels, bottom, Under::Points);
    bottom + below + PAD - c.top
}
