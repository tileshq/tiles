//! The calendar heatmap: a grid of cells shaded by value.

use skia_safe::Rect;
use tiles_intent::{Intent, IntentError};

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, Under, f};
use crate::format::NumberFormat;
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// Most rows the grid draws: the days of a month.
pub(crate) const MAX_GRID_ROWS: usize = 31;

/// A cell's height bounds: square cells within them.
const CELL_HEIGHT: (f32, f32) = (14.0, 30.0);

/// Space between two cells.
const CELL_GAP: f32 = 2.0;

/// A cell's corner radius, as the gallery rounds them.
const CELL_RADIUS: f32 = 2.0;

/// Shades in the key under the grid.
const KEY_STEPS: usize = 5;

/// A key swatch's size, and the space between the key's parts.
const KEY_SWATCH: (f32, f32) = (16.0, 8.0);
const KEY_GAP: f32 = 6.0;

/// Space between the column labels and the key.
const KEY_TOP: f32 = 10.0;

/// The share of the accent above which a cell's value is written in the
/// card's colour rather than ink.
const DARK_CELL: f32 = 0.55;

/// Refuses more rows than the grid draws.
pub(super) fn check(intent: &Intent, row: usize) -> Result<(), IntentError> {
    let mut seen: Vec<String> = Vec::new();
    for r in &intent.table.rows {
        let l = r[row].label();
        if !seen.contains(&l) {
            seen.push(l);
            if seen.len() > MAX_GRID_ROWS {
                return Err(IntentError::new(
                    "data.rows",
                    format!(
                        "more than {MAX_GRID_ROWS} distinct rows, more than form calendar-heatmap \
                         draws"
                    ),
                    "aggregate the rows, or swap the fields x and row",
                ));
            }
        }
    }
    Ok(())
}

/// Where a value sits from the lowest to the highest: linear, so the peaks
/// a title names stand out in every theme (a square-root scale, as the
/// gallery's sketch uses, left the peaks barely darker than the rest on a
/// dark card).
fn share(v: f64, lo: f64, hi: f64) -> f32 {
    if hi <= lo {
        return 1.0;
    }
    #[allow(clippy::cast_possible_truncation)] // A share, 0 to 1.
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
    t
}

/// A cell's shade at share `t`: the accent's opacity over the track, from
/// a floor so the lowest cell with data still differs from an empty one to
/// full at [`ACCENT_AT`], and above that ink laid over the accent up to
/// [`INK_TOP`], which lightens the top end on a dark card and deepens it on
/// a light one, so the ramp has more steps than the accent alone gives.
fn shade(t: f32) -> (f32, f32) {
    let accent = SHADE_FLOOR + (1.0 - SHADE_FLOOR) * (t / ACCENT_AT).min(1.0);
    let ink = ((t - ACCENT_AT) / (1.0 - ACCENT_AT)).max(0.0) * INK_TOP;
    (accent, ink)
}

/// The least share of the accent a cell with data takes.
const SHADE_FLOOR: f32 = 0.2;

/// The share at which the accent is full, and the most ink laid over it.
const ACCENT_AT: f32 = 0.8;
const INK_TOP: f32 = 0.35;

/// A grid of columns (x) by rows (row), each cell the track's shade with
/// the accent laid over it by value, from zero, or the lowest value when
/// one is below zero, to the highest. A highlight names rows, columns or
/// both; the cells where they meet carry their values and the names they
/// sit under are in ink. A key under the grid runs over the same range.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn heatmap(c: &mut Ctx) -> f32 {
    let xs = c.distinct("x");
    let rs = c.distinct("row");
    let (xc, rc, yc) = (
        c.col("x").unwrap_or(0),
        c.col("row").unwrap_or(0),
        c.col("y").unwrap_or(0),
    );
    // grid[row][column], laid out as `super::Cells` is.
    let mut grid: super::Cells = vec![vec![None; xs.len()]; rs.len()];
    for (r, row) in c.intent.table.rows.iter().enumerate() {
        let Some(v) = row[yc].number() else { continue };
        let xi = xs.iter().position(|l| *l == row[xc].label()).unwrap_or(0);
        let ri = rs.iter().position(|l| *l == row[rc].label()).unwrap_or(0);
        let cell = grid[ri][xi].get_or_insert((0.0, Vec::new()));
        cell.0 += v;
        cell.1.push(r);
    }
    let values: Vec<f64> = grid.iter().flatten().flatten().map(|(v, _)| *v).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // Counts shade from zero; signed values from the lowest.
    let lo = if min >= 0.0 { 0.0 } else { min };
    let ls = c.label_style();
    let vs = c.value_style();
    let row_names = c.fit_all(&rs, ls, LABEL_SHARE);
    let row_hl: Vec<bool> = rs.iter().map(|r| c.is_highlight(r)).collect();
    let col_hl: Vec<bool> = xs.iter().map(|x| c.is_highlight(x)).collect();
    let (any_row, any_col) = (row_hl.iter().any(|h| *h), col_hl.iter().any(|h| *h));
    let named = |ri: usize, xi: usize| {
        (any_row || any_col) && (!any_row || row_hl[ri]) && (!any_col || col_hl[xi])
    };
    let label_w = row_names
        .iter()
        .zip(&row_hl)
        .map(|(l, h)| c.width_of(l, if *h { ls.with_weight(600) } else { ls }))
        .fold(0.0, f32::max);
    let left = c.left + PAD + label_w + c.t.marks.label_padding;
    let right = c.left + c.width - PAD;
    let cell_w = (right - left) / f(xs.len().max(1));
    let cell_h = cell_w.clamp(CELL_HEIGHT.0, CELL_HEIGHT.1);
    let top = c.top + PAD;
    let muted = (Emphasis::Muted, c.pal.muted);
    let ink = (Emphasis::Ink, c.pal.ink);
    for (ri, name) in row_names.iter().enumerate() {
        let cy = top + cell_h * (f(ri) + 0.5);
        let (style, color) = if row_hl[ri] {
            (ls.with_weight(600), ink)
        } else {
            (ls, muted)
        };
        c.text(
            Part::AxisLabel,
            name.clone(),
            left - c.t.marks.label_padding,
            cy + (MIDDLE * style.size).round(),
            style,
            Anchor::End,
            color,
        );
    }
    let (accent_e, accent) = (Emphasis::Accent, c.pal.accent);
    for (ri, cells) in grid.iter().enumerate() {
        for (xi, cell) in cells.iter().enumerate() {
            let rect = Rect::from_xywh(
                left + cell_w * f(xi) + CELL_GAP / 2.0,
                top + cell_h * f(ri) + CELL_GAP / 2.0,
                (cell_w - CELL_GAP).max(1.0),
                (cell_h - CELL_GAP).max(1.0),
            );
            let shape = Shape::Rect {
                rect,
                radius: CELL_RADIUS,
                corners: Corners::All,
            };
            c.items.push(Item::new(
                Part::Guide,
                shape.clone(),
                Emphasis::Track,
                c.pal.track,
            ));
            let Some((v, rows)) = cell else { continue };
            let t = share(*v, lo, max);
            let (a, lift) = shade(t);
            let hl = named(ri, xi);
            let tip = c.tip(rows, &nf);
            c.items.push(
                Item::new(Part::Mark, shape.clone(), accent_e, accent.with_opacity(a))
                    .datum(rows[0], tip)
                    .highlighted(hl)
                    .grow(Grow::Fade),
            );
            if lift > 0.0 {
                c.items.push(
                    Item::new(
                        Part::Guide,
                        shape,
                        Emphasis::Ink,
                        c.pal.ink.with_opacity(lift),
                    )
                    .grow(Grow::Fade),
                );
            }
            if hl {
                let text = nf.format(*v);
                if c.width_of(&text, vs) + 4.0 <= rect.width() && vs.size + 2.0 <= rect.height() {
                    let color = if a > DARK_CELL {
                        (Emphasis::Card, c.pal.card)
                    } else {
                        ink
                    };
                    let at = c.items.len();
                    c.text(
                        Part::Value,
                        text,
                        rect.center_x(),
                        rect.center_y() + (MIDDLE * vs.size).round(),
                        vs,
                        Anchor::Middle,
                        color,
                    );
                    c.items[at].grow = Grow::Fade;
                }
            }
        }
    }
    let bottom = top + cell_h * f(rs.len());
    let labels: Vec<(String, f32)> = xs
        .iter()
        .enumerate()
        .map(|(i, x)| (x.clone(), left + cell_w * (f(i) + 0.5)))
        .collect();
    // Evenly thinned, every n-th column named, as a value axis keeps its
    // step: hours read 07:00, 09:00, 11:00, not whichever happen to fit.
    let below = c.bottom_labels(&labels, bottom, Under::Values);
    // The key: the low end, the shades from light to full, the highest.
    let key_top = bottom + below + KEY_TOP;
    let key_mid = key_top + KEY_SWATCH.1 / 2.0;
    let lo_text = nf.format(lo);
    let hi_text = nf.format(max);
    let mut kx = left;
    c.text(
        Part::Legend,
        lo_text.clone(),
        kx,
        key_mid + (MIDDLE * ls.size).round(),
        ls,
        Anchor::Start,
        muted,
    );
    kx += c.width_of(&lo_text, ls) + KEY_GAP;
    for k in 0..KEY_STEPS {
        let (a, lift) = shade(f(k) / f(KEY_STEPS - 1));
        let rect = Rect::from_xywh(kx, key_top, KEY_SWATCH.0, KEY_SWATCH.1);
        let shape = Shape::Rect {
            rect,
            radius: CELL_RADIUS,
            corners: Corners::All,
        };
        c.items.push(Item::new(
            Part::Legend,
            shape.clone(),
            Emphasis::Track,
            c.pal.track,
        ));
        c.items.push(Item::new(
            Part::Legend,
            shape.clone(),
            accent_e,
            accent.with_opacity(a),
        ));
        if lift > 0.0 {
            c.items.push(Item::new(
                Part::Legend,
                shape,
                Emphasis::Ink,
                c.pal.ink.with_opacity(lift),
            ));
        }
        kx += KEY_SWATCH.0 + CELL_GAP;
    }
    c.text(
        Part::Legend,
        hi_text,
        kx + KEY_GAP - CELL_GAP,
        key_mid + (MIDDLE * ls.size).round(),
        ls,
        Anchor::Start,
        muted,
    );
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    key_top + KEY_SWATCH.1 + PAD - c.top
}

#[cfg(test)]
mod tests {
    use super::{SHADE_FLOOR, shade, share};

    #[test]
    fn a_larger_value_is_never_a_lighter_shade() {
        let mut last = (0.0, 0.0);
        for k in 0..=100 {
            let s = shade(share(f64::from(k), 0.0, 100.0));
            assert!(s.0 >= last.0 && s.1 >= last.1, "{k}: {s:?} after {last:?}");
            last = s;
        }
        assert_eq!(
            shade(0.0),
            (SHADE_FLOOR, 0.0),
            "the lowest cell still shows"
        );
        assert!((shade(1.0).0 - 1.0).abs() < 1e-6 && shade(1.0).1 > 0.0);
        assert!(
            (share(5.0, 5.0, 5.0) - 1.0).abs() < 1e-6,
            "one value is the peak"
        );
    }
}
