//! Small multiples: one small line panel per series on a shared scale.

use skia_safe::Rect;
use tiles_intent::IntentError;

use super::{Cells, Ctx, MIDDLE, PAD, TOP, cells, f};
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, points, ticks};
use crate::scene::{Anchor, Emphasis, Grow, Item, Part, Shape};

/// Most panels the form draws.
pub(crate) const MAX_PANELS: usize = 12;

/// A panel's plot height, as the gallery's small multiples set it.
const PANEL_HEIGHT: f32 = 80.0;

/// Narrowest panel worth drawing, which sets how many sit in a row.
const MIN_PANEL_WIDTH: f32 = 150.0;

/// Space between two panels across and down.
const GUTTER: (f32, f32) = (20.0, 14.0);

/// Space under a panel's name.
const NAME_GAP: f32 = 6.0;

/// The context lines' stroke, faint behind each panel's own.
const CONTEXT_WIDTH: f32 = 1.5;

/// Ticks on each panel's value axis: its ends and its middle.
const TICKS: f64 = 2.0;

/// Refuses more panels than the form draws.
pub(super) fn check(series: &[String]) -> Result<(), IntentError> {
    if series.len() > MAX_PANELS {
        return Err(IntentError::new(
            "fields.series",
            format!(
                "{} panels is more than form small-multiples draws ({MAX_PANELS})",
                series.len()
            ),
            format!(
                "keep the {} that matter and group the rest as Other, or use multi-line",
                MAX_PANELS - 1
            ),
        ));
    }
    Ok(())
}

/// One small panel per series, in data order and as many to a row as fit,
/// sharing one scale: each panel's own line over every other series in
/// faint grey, its name above it, its last value at its end, its value
/// axis on its left and its first and last periods under it. A named panel
/// takes the accent (red for a bad finding) and the rest are muted; with
/// none named, every panel takes the accent.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn multiples(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let series = c.distinct("series");
    let grid: Cells = cells(c, &xs, &series);
    let values: Vec<f64> = grid.iter().flatten().flatten().map(|(v, _)| *v).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let vs = c.row_value_style();
    let name_style = ls.with_weight(600);
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let from = if min >= 0.0 && min <= max / 2.0 {
        0.0
    } else {
        min
    };
    let (lo, hi) = nice(from, max, TICKS);
    let value_ticks = ticks(lo, hi, TICKS);
    let tf = c.tick_format(&value_ticks, true);
    let tick_w = value_ticks
        .iter()
        .map(|v| c.width_of(&tf.format(*v), ls))
        .fold(0.0, f32::max);
    let value_w = values
        .iter()
        .map(|v| c.width_of(&nf.format(*v), vs))
        .fold(0.0, f32::max);
    let n = series.len().max(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // A few panels.
    let fit = ((c.width + GUTTER.0) / (MIN_PANEL_WIDTH + GUTTER.0))
        .floor()
        .max(1.0) as usize;
    // As many to a row as fit, then fewer while that adds no row, so four
    // panels sit two by two rather than three and one.
    let mut cols = fit.min(n);
    while cols > 1 && n.div_ceil(cols - 1) == n.div_ceil(cols) {
        cols -= 1;
    }
    let rows = n.div_ceil(cols);
    let panel_w = (c.width - GUTTER.0 * f(cols - 1)) / f(cols);
    // The name, then room for the top tick label's upper half.
    let name_h = (name_style.size + NAME_GAP + ls.size * 0.5).ceil();
    let below_h = c.t.marks.label_padding + ls.size + 4.0;
    let panel_h = name_h + PANEL_HEIGHT + below_h;
    let hl: Vec<bool> = series.iter().map(|s| c.is_highlight(s)).collect();
    let any = hl.iter().any(|h| *h);
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
    let muted = (Emphasis::Muted, c.pal.muted);
    let mut plot = Rect::from_ltrb(f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (si, name) in series.iter().enumerate() {
        let (col_i, row_i) = (si % cols, si / cols);
        let px = c.left + (panel_w + GUTTER.0) * f(col_i);
        let py = c.top + PAD + (panel_h + GUTTER.1) * f(row_i);
        let left = px + tick_w + c.t.marks.label_padding;
        let right = (px + panel_w - value_w - c.t.marks.row_value_offset).max(left + 1.0);
        let top = py + name_h;
        let y = Linear {
            d0: lo,
            d1: hi,
            r0: top + PANEL_HEIGHT,
            r1: top,
        };
        plot.join(Rect::from_ltrb(left, top, right, top + PANEL_HEIGHT));
        let shown = c.fit_all(std::slice::from_ref(name), name_style, panel_w / c.width);
        let (name_e, name_c) = if hl[si] {
            c.named()
        } else {
            (Emphasis::Ink, c.pal.ink)
        };
        c.text(
            Part::Legend,
            shown[0].clone(),
            px,
            py + (TOP * name_style.size).round(),
            name_style,
            Anchor::Start,
            (name_e, name_c),
        );
        for v in &value_ticks {
            let gy = y.map(*v);
            let line = Shape::Line {
                points: vec![(left, gy), (right, gy)],
                width: c.t.marks.grid_width,
            };
            c.items
                .push(Item::new(Part::Grid, line, Emphasis::Grid, c.pal.grid));
            c.text(
                Part::AxisLabel,
                tf.format(*v),
                left - c.t.marks.label_padding,
                gy + (MIDDLE * ls.size).round(),
                ls,
                Anchor::End,
                muted,
            );
        }
        let xpos = points(xs.len(), left, right, 0.0);
        let path = |k: usize| -> Vec<(f32, f32)> {
            grid.iter()
                .enumerate()
                .filter_map(|(xi, cells)| cells[k].as_ref().map(|(v, _)| (xpos[xi], y.map(*v))))
                .collect()
        };
        for k in (0..series.len()).filter(|k| *k != si) {
            let pts = path(k);
            if pts.len() > 1 {
                let shape = Shape::Line {
                    points: pts,
                    width: CONTEXT_WIDTH,
                };
                c.items.push(
                    Item::new(Part::Guide, shape, Emphasis::Grid, c.pal.grid).grow(Grow::Wipe),
                );
            }
        }
        let (e, col) = if !any {
            (Emphasis::Accent, c.pal.accent)
        } else if hl[si] {
            c.named()
        } else {
            muted
        };
        let own = path(si);
        let first_row = grid.iter().find_map(|cells| {
            cells[si]
                .as_ref()
                .and_then(|(_, rows)| rows.first().copied())
        });
        if own.len() > 1 {
            let mut item = Item::new(
                Part::Mark,
                Shape::Line {
                    points: own.clone(),
                    width: c.t.marks.line_width,
                },
                e,
                col,
            )
            .highlighted(hl[si])
            .grow(Grow::Wipe);
            item.datum = first_row;
            c.items.push(item);
        }
        // Each period answers hover; the last carries a point and the value.
        for (xi, cells) in grid.iter().enumerate() {
            let Some((v, rows)) = cells[si].as_ref() else {
                continue;
            };
            let tip = format!(
                "{s_name}: {name}\n{x_name}: {}\n{y_name}: {}",
                xs[xi],
                nf.format(*v)
            );
            let shape = Shape::Circle {
                cx: xpos[xi],
                cy: y.map(*v),
                r: c.t.marks.point_radius,
            };
            c.items.push(
                Item::new(Part::Hit, shape, e, col)
                    .datum(rows[0], tip)
                    .highlighted(hl[si]),
            );
        }
        if let Some((xi, v)) = grid
            .iter()
            .enumerate()
            .rev()
            .find_map(|(xi, cells)| cells[si].as_ref().map(|(v, _)| (xi, *v)))
        {
            let (cx, cy) = (xpos[xi], y.map(v));
            let tip = format!(
                "{s_name}: {name}\n{x_name}: {}\n{y_name}: {}",
                xs[xi],
                nf.format(v)
            );
            let rows = grid[xi][si].as_ref().map_or(0, |(_, rows)| rows[0]);
            c.items.push(
                Item::new(
                    Part::Mark,
                    Shape::Circle {
                        cx,
                        cy,
                        r: c.t.marks.point_radius,
                    },
                    e,
                    col,
                )
                .datum(rows, tip)
                .highlighted(hl[si])
                .grow(Grow::Wipe),
            );
            let at = c.items.len();
            let vc = if hl[si] || !any {
                (Emphasis::Ink, c.pal.ink)
            } else {
                muted
            };
            c.text(
                Part::Value,
                nf.format(v),
                cx + c.t.marks.row_value_offset,
                cy + (MIDDLE * vs.size).round(),
                vs,
                Anchor::Start,
                vc,
            );
            c.items[at].grow = Grow::Fade;
        }
        // The first and last periods under the panel, as the gallery's
        // panels name their span.
        let baseline = top + PANEL_HEIGHT + c.t.marks.label_padding + (TOP * ls.size).round();
        let ends: Vec<usize> = if xs.len() > 1 {
            vec![0, xs.len() - 1]
        } else {
            vec![0]
        };
        let labels = super::month_ticks(
            &ends
                .iter()
                .map(|&i| (xs[i].clone(), xpos[i]))
                .collect::<Vec<_>>(),
            false,
        )
        .unwrap_or_else(|| ends.iter().map(|&i| (xs[i].clone(), xpos[i])).collect());
        let shown = c.fit_all(
            &labels.iter().map(|l| l.0.clone()).collect::<Vec<_>>(),
            ls,
            (right - left) / 2.0 / c.width,
        );
        for (k, ((_, lx), text)) in labels.iter().zip(shown).enumerate() {
            let anchor = if k == 0 && labels.len() > 1 {
                Anchor::Start
            } else {
                Anchor::End
            };
            c.text(Part::AxisLabel, text, *lx, baseline, ls, anchor, muted);
        }
    }
    c.plot = plot;
    PAD + panel_h * f(rows) + GUTTER.1 * f(rows - 1) + PAD
}
