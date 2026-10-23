//! Shares of a whole.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, TOP, X_LABEL_NUDGE, cells, f, series_colors};
use crate::color::Rgba;
use crate::format::NumberFormat;
use crate::scale::{Band, Linear};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The share bar's plot height, as the gallery's 100% bar sets it.
const SHARES_HEIGHT: f32 = 200.0;

/// The share bar's thickness as a share of its band, as the gallery sets it.
const SHARES_BAND: f32 = 0.72;

/// The card-coloured seam between two segments, as the gallery's stroke.
const SHARES_SEAM: f32 = 1.5;

/// How far the legend sits below the chart's top, and the room it leaves
/// over the bars past its own rows, as Vega's top legend placed them.
const SHARES_LEGEND: (f32, f32) = (4.0, 9.0);

/// The smallest share, in percent, written inside its segment.
const SHARES_LABELLED: f64 = 10.0;

/// A share in whole percent. One that is neither 0 nor 100 but rounds to
/// it reads `<1` or `>99`, so a segment too thin to see still says it is
/// there.
fn share_text(pf: &NumberFormat, pct: f64) -> String {
    let r = pct.round();
    if r <= 0.0 && pct > 0.0 {
        "<1".to_string()
    } else if r >= 100.0 && pct < 100.0 {
        ">99".to_string()
    } else {
        pf.format(r)
    }
}

/// Horizontal bars of shares stacked to 100%, one per category, each
/// segment's share written inside it, under a legend of square swatches and
/// over a 0%, 50%, 100% axis (playbook M12). With a named series, rows run
/// from its largest share down.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn shares(c: &mut Ctx) -> f32 {
    let mut xs = c.distinct("x");
    let series = c.distinct("series");
    let mut grid = cells(c, &xs, &series);
    let (colors, series_hl) = series_colors(c, &series, true);
    let total =
        |row: &[Option<(f64, Vec<usize>)>]| -> f64 { row.iter().flatten().map(|(v, _)| *v).sum() };
    let share = |row: &[Option<(f64, Vec<usize>)>], si: usize| -> f64 {
        let t = total(row);
        match &row[si] {
            Some((v, _)) if t > 0.0 => v / t * 100.0,
            _ => 0.0,
        }
    };
    if let Some(named) = series_hl.iter().position(|h| *h) {
        let mut order: Vec<usize> = (0..xs.len()).collect();
        order.sort_by(|a, b| share(&grid[*b], named).total_cmp(&share(&grid[*a], named)));
        xs = order.iter().map(|&i| xs[i].clone()).collect();
        grid = order.iter().map(|&i| grid[i].clone()).collect();
    }
    let ls = c.label_style();
    let vs = c.value_style();
    let entries: Vec<(String, (Emphasis, Rgba))> =
        series.iter().cloned().zip(colors.iter().copied()).collect();
    let legend = c.legend_with(&entries, c.top + SHARES_LEGEND.0, true) + SHARES_LEGEND.1;
    let shown = c.fit_all(&xs, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    let left = c.left + PAD + widest + c.t.marks.label_padding;
    let right = c.left + c.width - PAD;
    let x = Linear {
        d0: 0.0,
        d1: 100.0,
        r0: left,
        r1: right,
    };
    let inner = c.t.marks.band_padding_inner;
    let outer = c.t.marks.band_padding_outer;
    let span_of = |step: f32| step * (f(xs.len()) - inner + 2.0 * outer);
    let step = c.t.marks.row_step.max(SHARES_HEIGHT / span_of(1.0));
    let top = c.top + PAD + legend;
    let span = span_of(step);
    let band = Band::new(xs.len(), top, top + span, inner, outer);
    let pf = NumberFormat::for_values(&[0.0]);
    for (xi, row) in grid.iter().enumerate() {
        let cy = band.center(xi);
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::AxisLabel,
            shown[xi].clone(),
            left - c.t.marks.label_padding,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            muted,
        );
        let thick = band.width * SHARES_BAND;
        let mut acc = 0.0;
        let last = row
            .iter()
            .rposition(|cell| cell.as_ref().is_some_and(|(v, _)| *v > 0.0));
        let nf = NumberFormat::for_values(&[total(row)]).with_unit(c.intent.unit);
        for (si, cell) in row.iter().enumerate() {
            let Some((v, rows)) = cell else { continue };
            if *v <= 0.0 {
                continue;
            }
            let pct = share(row, si);
            let (x0, x1) = (x.map(acc), x.map(acc + pct));
            acc += pct;
            // Each segment gives up half the seam on each inner side.
            let seam = SHARES_SEAM / 2.0;
            let l = if acc - pct > 0.0 { x0 + seam } else { x0 };
            let r = if Some(si) == last { x1 } else { x1 - seam };
            let (e, col) = colors[si];
            let rect = Rect::from_ltrb(l, cy - thick / 2.0, r.max(l), cy + thick / 2.0);
            let tip = format!("{}\nshare: {}%", c.tip(rows, &nf), share_text(&pf, pct));
            let shape = Shape::Rect {
                rect,
                radius: 0.0,
                corners: Corners::All,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(rows[0], tip)
                    .highlighted(series_hl[si])
                    .grow(Grow::FromX(left)),
            );
            let text = share_text(&pf, pct);
            if pct >= SHARES_LABELLED && c.width_of(&text, vs) + 4.0 <= r - l {
                // Card-coloured type on the accent, ink on the neutrals, as
                // the gallery sets them; on a categorical colour, whichever
                // reads the stronger.
                let on_card = match e {
                    Emphasis::Accent | Emphasis::Bad => true,
                    Emphasis::Neutral(_) => false,
                    _ => col.contrast(c.pal.card) > col.contrast(c.pal.ink),
                };
                let ink = if on_card {
                    (Emphasis::Card, c.pal.card)
                } else {
                    (Emphasis::Ink, c.pal.ink)
                };
                let at = c.items.len();
                c.text(
                    Part::Value,
                    text,
                    f32::midpoint(l, r),
                    cy + (MIDDLE * vs.size).round(),
                    vs,
                    Anchor::Middle,
                    ink,
                );
                c.items[at].grow = Grow::Fade;
            }
        }
    }
    let bottom = top + span;
    c.plot = Rect::from_ltrb(left, top, right, bottom);
    // The axis's end labels sit flush with the plot's ends.
    let baseline = bottom + c.t.marks.label_padding + X_LABEL_NUDGE + (TOP * ls.size).round();
    for (v, anchor) in [
        (0.0, Anchor::Start),
        (50.0, Anchor::Middle),
        (100.0, Anchor::End),
    ] {
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::AxisLabel,
            format!("{}%", pf.format(v)),
            x.map(v),
            baseline,
            ls,
            anchor,
            muted,
        );
    }
    bottom + c.t.marks.label_padding + ls.size + PAD - c.top
}
