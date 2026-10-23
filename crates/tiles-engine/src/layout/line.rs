//! The line chart.

use skia_safe::Rect;

use super::{Cells, Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f, overlaps, series_colors, spread};
use crate::color::Rgba;
use crate::format::NumberFormat;
use crate::scale::{Linear, nice, points};
use crate::scene::{Anchor, Emphasis, Grow, Item, Part, Shape};

/// Space between a line's end and its name.
const END_LABEL_GAP: f32 = 5.0;

/// Lines with points across ordered periods; one or several series,
/// optionally filled from zero (playbook M4).
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn line(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let xcol = c.col("x").unwrap_or(0);
    let ycol = c.col("y").unwrap_or(0);
    let scol = c.col("series");
    let series: Vec<String> = if scol.is_some() {
        c.distinct("series")
    } else {
        vec![String::new()]
    };
    // values[s][x] = (sum, rows)
    let mut grid: Cells = vec![vec![None; xs.len()]; series.len()];
    for (r, row) in c.intent.table.rows.iter().enumerate() {
        let Some(v) = row[ycol].number() else {
            continue;
        };
        let xi = xs.iter().position(|l| *l == row[xcol].label()).unwrap_or(0);
        let si = scol.map_or(0, |s| {
            series
                .iter()
                .position(|l| *l == row[s].label())
                .unwrap_or(0)
        });
        let cell = grid[si][xi].get_or_insert((0.0, Vec::new()));
        cell.0 += v;
        cell.1.push(r);
    }
    let values: Vec<f64> = grid.iter().flatten().flatten().map(|(v, _)| *v).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let area = c.form.options.area;
    let labelled = c.form.options.label_highlight;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = if area {
        nice(min.min(0.0), max.max(0.0), 10.0)
    } else {
        nice(min, max, 10.0)
    };
    let h = c.t.marks.plot_height;
    let count = (h / c.t.marks.tick_spacing).ceil();
    let tick_w = c.tick_label_width(lo, hi, count);
    let left = c.left + PAD + tick_w + c.t.marks.label_padding;
    // Several lines are named at their ends, as the gallery labels them,
    // when the names fit down the plot; otherwise a legend names them.
    let ls = c.label_style();
    let names = c.fit_all(&series, ls.with_weight(600), LABEL_SHARE / 2.0);
    let direct = scol.is_some() && f(series.len()) * (ls.size + 2.0) <= h;
    let name_room = if direct {
        names
            .iter()
            .map(|n| c.width_of(n, ls.with_weight(600)))
            .fold(0.0, f32::max)
            + END_LABEL_GAP
    } else {
        0.0
    };
    let right = c.left + c.width - PAD - name_room;

    let (colors, series_hl) = if scol.is_some() {
        series_colors(c, &series, false)
    } else {
        (vec![(Emphasis::Accent, c.pal.accent)], vec![false])
    };
    let mut top = c.top + PAD;
    if scol.is_some() && !direct {
        let entries: Vec<(String, (Emphasis, Rgba))> =
            series.iter().cloned().zip(colors.iter().copied()).collect();
        top += c.legend(&entries, c.top);
    }
    top += (c.label_style().size * 0.5).ceil();
    let y = Linear {
        d0: lo,
        d1: hi,
        r0: top + h,
        r1: top,
    };
    c.y_axis(&y, None, left, right);
    let xpos = points(xs.len(), left, right, 0.5);
    let base = y.map(lo.max(0.0).min(hi));
    // Paint highlighted series last, so they sit on top.
    let mut order: Vec<usize> = (0..series.len()).collect();
    order.sort_by_key(|&i| series_hl[i]);
    let many = xs.len() > 40;
    // A spotlight on one of several lines draws it firm and the rest thin,
    // pointless, as the gallery's highlighted line does.
    let spotlight = scol.is_some() && series_hl.iter().any(|h| *h);
    for &si in &order {
        let pts: Vec<(f32, f32)> = grid[si]
            .iter()
            .enumerate()
            .filter_map(|(xi, cell)| cell.as_ref().map(|(v, _)| (xpos[xi], y.map(*v))))
            .collect();
        let (e, col) = colors[si];
        if area {
            let shape = Shape::Area {
                points: pts.clone(),
                base,
            };
            c.items.push(
                Item::new(
                    Part::Mark,
                    shape,
                    e,
                    col.with_opacity(c.t.marks.area_opacity),
                )
                .highlighted(series_hl[si])
                .grow(Grow::Wipe),
            );
        }
        // The approved area has no edge stroke; the labelled variant keeps one.
        if pts.len() > 1 && (!area || labelled) {
            // A lone line sits a touch under full strength, as the prototype's does.
            let alpha = if scol.is_some() {
                1.0
            } else {
                c.t.marks.line_opacity
            };
            let width = match (spotlight, series_hl[si]) {
                (true, true) => c.t.marks.line_width_named,
                (true, false) => c.t.marks.line_width_rest,
                _ => c.t.marks.line_width,
            };
            let shape = Shape::Line { points: pts, width };
            let mut item = Item::new(Part::Mark, shape, e, col.with_opacity(alpha))
                .highlighted(series_hl[si])
                .grow(Grow::Wipe);
            // The line draws its series: its first row, without hover text,
            // since each point carries the hover.
            item.datum = grid[si]
                .iter()
                .flatten()
                .find_map(|(_, rows)| rows.first().copied());
            c.items.push(item);
        }
        for (xi, cell) in grid[si].iter().enumerate() {
            let Some((v, rows)) = cell else { continue };
            let point_hl = c.is_highlight(&xs[xi]) || series_hl[si];
            let named_point = c.is_highlight(&xs[xi]);
            let tip = c.tip(rows, &nf);
            if (many || spotlight) && !named_point {
                let shape = Shape::Circle {
                    cx: xpos[xi],
                    cy: y.map(*v),
                    r: c.t.marks.point_radius,
                };
                c.items.push(
                    Item::new(Part::Hit, shape, e, col)
                        .datum(rows[0], tip)
                        .highlighted(point_hl),
                );
                continue;
            }
            // Every period gets its point; a named one gets a larger point
            // laid over it, both translucent, as the prototype layers them.
            let mut dots = vec![(c.t.marks.point_radius, (e, col))];
            if named_point {
                dots.push((c.t.marks.point_radius_highlight, c.named()));
            }
            for (r, (pe, pc)) in dots {
                let shape = Shape::Circle {
                    cx: xpos[xi],
                    cy: y.map(*v),
                    r,
                };
                c.items.push(
                    Item::new(
                        Part::Mark,
                        shape,
                        pe,
                        pc.with_opacity(c.t.marks.point_opacity),
                    )
                    .datum(rows[0], tip.clone())
                    .highlighted(point_hl)
                    .grow(Grow::Wipe),
                );
            }
            if named_point && labelled {
                let r = c.t.marks.point_radius_highlight;
                let (pe, pc) = c.named();
                let vs = c.value_style();
                let at = c.items.len();
                let vy = y.map(*v) - r - c.t.marks.value_offset + (MIDDLE * vs.size).round();
                c.text(
                    Part::Value,
                    nf.format(*v),
                    xpos[xi],
                    vy,
                    vs,
                    Anchor::Middle,
                    (pe, pc),
                );
                c.items[at].grow = Grow::Fade;
            }
        }
    }
    let spotlight_any = series_hl.iter().any(|h| *h);
    let mut ends: Vec<(usize, f32)> = Vec::new();
    for (si, row) in grid.iter().enumerate() {
        if let Some((v, _)) = row.iter().rev().find_map(Option::as_ref) {
            ends.push((si, y.map(*v)));
        }
    }
    if direct {
        spread(&mut ends, ls.size + 2.0, (top, top + h));
        for &(si, yy) in &ends {
            let named = series_hl[si];
            let (style, color) = if named {
                (ls.with_weight(600), colors[si])
            } else if spotlight_any {
                (ls, (Emphasis::Muted, c.pal.muted))
            } else {
                (ls, colors[si])
            };
            let at = c.items.len();
            c.text(
                Part::Legend,
                names[si].clone(),
                right + END_LABEL_GAP,
                yy + (MIDDLE * style.size).round(),
                style,
                Anchor::Start,
                color,
            );
            c.items[at].grow = Grow::Fade;
        }
    }
    if let Some(note) = c.intent.note.clone() {
        // Beside the named period's point, else the named series' last point.
        let at_point = xs.iter().position(|x| c.is_highlight(x)).and_then(|xi| {
            (0..series.len())
                .rev()
                .find_map(|si| grid[si][xi].as_ref().map(|(v, _)| (xpos[xi], y.map(*v))))
        });
        let at_series = series_hl.iter().position(|h| *h).and_then(|si| {
            grid[si]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(xi, cell)| cell.as_ref().map(|(v, _)| (xpos[xi], y.map(*v))))
        });
        if let Some((px, py)) = at_point.or(at_series) {
            let pr = c.t.marks.point_radius_highlight;
            let point = Rect::from_ltrb(px - pr, py - pr, px + pr, py + pr);
            let gap = c.t.marks.label_padding;
            // Clear of the points drawn and of every series' line, traced as
            // boxes as wide as the widest stroke every 2 px.
            let half = c.t.marks.line_width_named / 2.0;
            let paths: Vec<Vec<(f32, f32)>> = grid
                .iter()
                .map(|cells| {
                    cells
                        .iter()
                        .enumerate()
                        .filter_map(|(xi, cell)| cell.as_ref().map(|(v, _)| (xpos[xi], y.map(*v))))
                        .collect()
                })
                .collect();
            let mut marks: Vec<Rect> = c
                .items
                .iter()
                .filter(|i| i.part == Part::Mark && matches!(i.shape, Shape::Circle { .. }))
                .map(Item::bounds)
                .collect();
            marks.push(point);
            for pair in paths.iter().flat_map(|p| p.windows(2)) {
                let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
                let steps = ((x1 - x0).hypot(y1 - y0) / 2.0).ceil().max(1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // A few hundred steps.
                for k in 0..=steps as u32 {
                    #[allow(clippy::cast_precision_loss)]
                    let t = k as f32 / steps;
                    let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
                    marks.push(Rect::from_ltrb(x - half, y - half, x + half, y + half));
                }
            }
            let spots = |c: &Ctx, w: f32| {
                c.note_spots(w, point, gap, (left, right))
                    .into_iter()
                    .map(|(x, anchor)| (x, anchor, py - pr - gap))
                    .collect()
            };
            let rows = (top + ls.size, top + h - (0.2 * ls.size).ceil());
            // The point's own value, and its series' name at its end.
            let reach = point.with_outset((2.0 * gap, 2.0 * gap));
            let mut own = point;
            c.items
                .iter()
                .filter(|i| matches!(i.part, Part::Value | Part::Legend))
                .filter_map(|i| c.text_box(i))
                .filter(|b| overlaps(b, &reach))
                .for_each(|b| own.join(b));
            c.note(&note, spots, rows, &marks, (own, left));
        }
    }
    let bottom = top + h;
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    let labels: Vec<(String, f32)> = xs.iter().cloned().zip(xpos).collect();
    let below = c.bottom_labels(&labels, bottom, Under::Points);
    bottom + below + PAD - c.top
}
