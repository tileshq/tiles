//! The big number, and the compact scorecard among others.

use skia_safe::Rect;
use tiles_intent::{Tone, Unit};

use super::{COMPACT_SPARK, COMPACT_SPARK_GAP, Ctx, MIDDLE, PAD, f, symbol_radius, year_month};
use crate::format::NumberFormat;
use crate::scale::{Linear, points};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};
use crate::text::Style;

/// One big number over a sparkline of its rows, with an optional target
/// and its progress bar.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn kpi(c: &mut Ctx) -> f32 {
    if c.compact {
        return kpi_compact(c);
    }
    let ycol = c.col("y").unwrap_or(0);
    let rows: Vec<(usize, f64)> = c
        .intent
        .table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(r, row)| row[ycol].number().map(|v| (r, v)))
        .collect();
    let values: Vec<f64> = rows.iter().map(|(_, v)| *v).collect();
    let nf = NumberFormat::for_headline(&values).with_unit(c.intent.unit);
    let (last_row, last) = rows[rows.len() - 1];
    let tip = c.tip(&[last_row], &nf);
    let big = Style::new(c.t.text.big_size, c.t.text.big_weight).tabular();
    let top = c.top + PAD;
    // The number's middle sits 20 px down the chart, as the gallery sets it.
    let baseline = top + KPI_NUMBER_MIDDLE + (MIDDLE * big.size).round();
    let ink = (Emphasis::Ink, c.pal.ink);
    c.text(
        Part::Value,
        nf.format(last),
        c.left + PAD,
        baseline,
        big,
        Anchor::Start,
        ink,
    );
    let at = c.items.len() - 1;
    c.items[at].datum = Some(last_row);
    c.items[at].tip = Some(tip);
    let mut y = top + 2.0 * KPI_NUMBER_MIDDLE;
    let ls = c.label_style().tabular();
    let muted = (Emphasis::Muted, c.pal.muted);
    let (e, col) = if c.intent.claim.tone == Tone::Bad {
        (Emphasis::Bad, c.pal.bad)
    } else {
        (Emphasis::Accent, c.pal.accent)
    };
    let mut spark_top = top;
    if let Some(tc) = c.col("target")
        && let Some(target) = c.intent.table.rows[last_row][tc]
            .number()
            .filter(|t| *t > 0.0)
    {
        let share = last / target;
        let pf = NumberFormat::for_values(&[(share * 100.0).round()]);
        let text = format!(
            "{}% of the {} target",
            pf.format((share * 100.0).round()),
            nf.format(target)
        );
        y += 6.0;
        c.text(
            Part::Value,
            text,
            c.left + PAD,
            y + c.fonts.baseline_in_line(ls.size, 15.0),
            ls,
            Anchor::Start,
            muted,
        );
        y += 15.0 + 6.0;
        let w = c.width - 2.0 * PAD;
        let track = Shape::Rect {
            rect: Rect::from_xywh(c.left + PAD, y, w, 6.0),
            radius: 3.0,
            corners: Corners::All,
        };
        c.items
            .push(Item::new(Part::Guide, track, Emphasis::Track, c.pal.track));
        #[allow(clippy::cast_possible_truncation)] // A share of a card width.
        let fill = (share.clamp(0.0, 1.0) as f32) * w;
        let shape = Shape::Rect {
            rect: Rect::from_xywh(c.left + PAD, y, fill, 6.0),
            radius: 3.0,
            corners: Corners::All,
        };
        let tip = c.tip(&[last_row], &nf);
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(last_row, tip)
                .grow(Grow::FromX(c.left + PAD)),
        );
        y += 6.0;
        // The sparkline keeps its height and drops until its highest point
        // sits clear under the bar.
        let (under, over) = KPI_SPARK_ROOM;
        #[allow(clippy::cast_possible_truncation)] // A share of the chart height.
        let reach = KPI_HEIGHT * (over / (1.0 + under + over)) as f32;
        spark_top = (y + 10.0 - reach).max(top);
    }
    if rows.len() < 3 {
        return y + PAD - c.top;
    }
    // The sparkline fills the chart under and behind the number, its line
    // in the lower half and its fill down to the chart's foot, as the
    // gallery's big number draws it.
    let bottom = (spark_top + KPI_HEIGHT).max(y + c.t.marks.spark_height);
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = if max > min { max - min } else { 1.0 };
    let sy = Linear {
        d0: min - range * KPI_SPARK_ROOM.0,
        d1: max + range * KPI_SPARK_ROOM.1,
        r0: bottom,
        r1: spark_top,
    };
    let x0 = c.left + PAD;
    let w = c.width - 2.0 * PAD;
    // The last point stops short of the edge, as the gallery's 12 of 12.6 does.
    let reach = w / KPI_SPARK_REACH;
    let pts: Vec<(f32, f32)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (x0 + reach * f(i) / f(rows.len() - 1), sy.map(*v)))
        .collect();
    let shape = Shape::Area {
        points: pts.clone(),
        base: bottom,
    };
    c.items
        .push(Item::new(Part::Mark, shape, e, col.with_opacity(0.12)).grow(Grow::Wipe));
    let shape = Shape::Line {
        points: pts.clone(),
        width: 2.0,
    };
    c.items
        .push(Item::new(Part::Mark, shape, e, col).grow(Grow::Wipe));
    let end = symbol_radius(KPI_END_DOT);
    for (i, (px, py)) in pts.iter().enumerate() {
        let (row, _) = rows[i];
        let tip = c.tip(&[row], &nf);
        let last_point = i == pts.len() - 1;
        if last_point {
            // A ring in the card's colour, as the gallery's 1.5 px stroke.
            let ring = Shape::Circle {
                cx: *px,
                cy: *py,
                r: end + 0.75,
            };
            c.items
                .push(Item::new(Part::Guide, ring, Emphasis::Card, c.pal.card).grow(Grow::Wipe));
        }
        let radius = if last_point { end - 0.75 } else { 0.01 };
        let shape = Shape::Circle {
            cx: *px,
            cy: *py,
            r: radius,
        };
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(row, tip)
                .highlighted(last_point)
                .grow(Grow::Wipe),
        );
    }
    // Where the line starts, under its first point: "12 months ago" for
    // periods written YYYY-MM, counted from the first to the last, else the
    // first period's own label.
    if let Some(xc) = c.col("x") {
        let labels: Vec<String> = rows
            .iter()
            .map(|(r, _)| c.intent.table.rows[*r][xc].label())
            .collect();
        let months: Option<Vec<usize>> = labels
            .iter()
            .map(|l| year_month(l).map(|(y, m)| y as usize * 12 + m))
            .collect();
        let text = match months.as_deref() {
            Some([first, .., last]) if last > first => match last - first {
                1 => "1 month ago".to_string(),
                n => format!("{n} months ago"),
            },
            _ => labels[0].clone(),
        };
        let shade = (Emphasis::Neutral(0), c.pal.neutrals[0]);
        let at = c.items.len();
        let ls = c.label_style();
        c.text(
            Part::AxisLabel,
            text,
            pts[0].0,
            (pts[0].1 + 14.0 + (MIDDLE * ls.size).round()).min(bottom - 2.0),
            ls,
            Anchor::Start,
            shade,
        );
        c.items[at].grow = Grow::Fade;
    }
    c.plot = Rect::from_ltrb(x0, top, x0 + w, bottom);
    bottom + PAD - c.top
}

/// The big number's chart height, as the gallery's big number sets it.
const KPI_HEIGHT: f32 = 130.0;

/// How far down the chart the number's middle sits.
const KPI_NUMBER_MIDDLE: f32 = 20.0;

/// The sparkline's room under its lowest and over its highest value, as
/// shares of its range: the gallery's domain of 10 to 17.6 for 11.1 to 14.2.
const KPI_SPARK_ROOM: (f64, f64) = (0.355, 1.1);

/// The sparkline's end dot, as a Vega symbol size.
const KPI_END_DOT: f32 = 60.0;

/// How far short of the edge the last point stops: the gallery's 12 of 12.6.
const KPI_SPARK_REACH: f32 = 1.05;

/// A scorecard among others ([`crate::Align::compact`]): one big number,
/// its change against the previous period, an optional target and a short
/// sparkline under them.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
fn kpi_compact(c: &mut Ctx) -> f32 {
    let ycol = c.col("y").unwrap_or(0);
    let rows: Vec<(usize, f64)> = c
        .intent
        .table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(r, row)| row[ycol].number().map(|v| (r, v)))
        .collect();
    let values: Vec<f64> = rows.iter().map(|(_, v)| *v).collect();
    let nf = NumberFormat::for_headline(&values).with_unit(c.intent.unit);
    let (last_row, last) = rows[rows.len() - 1];
    let tip = c.tip(&[last_row], &nf);
    let big = Style::new(c.t.text.big_size, c.t.text.big_weight).tabular();
    let line_h = c.t.text.big_size * 1.15;
    let mut y = c.top + PAD;
    let baseline = y + c.fonts.baseline_in_line(big.size, line_h);
    let ink = (Emphasis::Ink, c.pal.ink);
    c.text(
        Part::Value,
        nf.format(last),
        c.left + PAD,
        baseline,
        big,
        Anchor::Start,
        ink,
    );
    let at = c.items.len() - 1;
    c.items[at].datum = Some(last_row);
    c.items[at].tip = Some(tip);
    y += line_h;

    let tone = match c.intent.claim.tone {
        Tone::Bad => (Emphasis::Bad, c.pal.bad),
        Tone::Good => (Emphasis::Accent, c.pal.accent),
        Tone::Neutral => (Emphasis::Muted, c.pal.muted),
    };
    let xcol = c.col("x");
    if rows.len() >= 2 {
        let (prev_row, prev) = rows[rows.len() - 2];
        let delta = last - prev;
        let points = c.intent.unit == Some(Unit::Percent);
        let mut text = if points {
            let plain = nf.with_unit(None);
            let one = plain.format(delta.abs()).parse::<f64>() == Ok(1.0);
            let unit = if one { "pt" } else { "pts" };
            format!("{} {unit}", plain.signed(delta))
        } else {
            nf.signed(delta)
        };
        if prev != 0.0 && !points {
            let pct = delta / prev.abs() * 100.0;
            let pf = NumberFormat::for_values(&[(pct * 10.0).round() / 10.0]);
            text = format!("{} ({}%)", text, pf.signed((pct * 10.0).round() / 10.0));
        }
        let vs = match xcol {
            Some(xc) => format!(" vs {}", c.intent.table.rows[prev_row][xc].label()),
            None => " vs the previous row".to_string(),
        };
        let ds = Style::new(c.t.text.delta_size, c.t.text.delta_weight).tabular();
        let base = y + 4.0 + c.fonts.baseline_in_line(ds.size, ds.size * 1.3);
        c.text(
            Part::Value,
            text.clone(),
            c.left + PAD,
            base,
            ds,
            Anchor::Start,
            tone,
        );
        let w = c.width_of(&text, ds);
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::Value,
            vs,
            c.left + PAD + w,
            base,
            Style::new(ds.size, 400).tabular(),
            Anchor::Start,
            muted,
        );
        y += 4.0 + ds.size * 1.3;
    }
    if let Some(tc) = c.col("target")
        && let Some(target) = c.intent.table.rows[last_row][tc]
            .number()
            .filter(|t| *t > 0.0)
    {
        let share = last / target;
        let pf = NumberFormat::for_values(&[(share * 100.0).round()]);
        let ls = c.label_style().tabular();
        y += 10.0;
        let text = format!(
            "{}% of the {} target",
            pf.format((share * 100.0).round()),
            nf.format(target)
        );
        let muted = (Emphasis::Muted, c.pal.muted);
        c.text(
            Part::Value,
            text,
            c.left + PAD,
            y + c.fonts.baseline_in_line(ls.size, 15.0),
            ls,
            Anchor::Start,
            muted,
        );
        y += 15.0 + 6.0;
        let w = c.width - 2.0 * PAD;
        let track = Rect::from_xywh(c.left + PAD, y, w, 6.0);
        let shape = Shape::Rect {
            rect: track,
            radius: 3.0,
            corners: Corners::All,
        };
        c.items
            .push(Item::new(Part::Guide, shape, Emphasis::Track, c.pal.track));
        #[allow(clippy::cast_possible_truncation)] // A share of a card width.
        let fill = (share.clamp(0.0, 1.0) as f32) * w;
        let (e, col) = if c.intent.claim.tone == Tone::Bad {
            (Emphasis::Bad, c.pal.bad)
        } else {
            (Emphasis::Accent, c.pal.accent)
        };
        let shape = Shape::Rect {
            rect: Rect::from_xywh(c.left + PAD, y, fill, 6.0),
            radius: 3.0,
            corners: Corners::All,
        };
        let tip = c.tip(&[last_row], &nf);
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(last_row, tip)
                .grow(Grow::FromX(c.left + PAD)),
        );
        y += 6.0;
    }
    if rows.len() >= 3 {
        y += COMPACT_SPARK_GAP;
        let h = COMPACT_SPARK;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let r = c.t.marks.point_radius;
        let sy = Linear {
            d0: min,
            d1: if max > min { max } else { min + 1.0 },
            r0: y + h - r,
            r1: y + r,
        };
        let xs = points(
            rows.len(),
            c.left + PAD + r,
            c.left + c.width - PAD - r,
            0.0,
        );
        let pts: Vec<(f32, f32)> = xs
            .iter()
            .zip(&values)
            .map(|(x, v)| (*x, sy.map(*v)))
            .collect();
        let (e, col) = if c.intent.claim.tone == Tone::Bad {
            (Emphasis::Bad, c.pal.bad)
        } else {
            (Emphasis::Accent, c.pal.accent)
        };
        let shape = Shape::Area {
            points: pts.clone(),
            base: y + h,
        };
        c.items
            .push(Item::new(Part::Mark, shape, e, col.with_opacity(0.12)).grow(Grow::Wipe));
        let shape = Shape::Line {
            points: pts.clone(),
            width: 2.0,
        };
        c.items
            .push(Item::new(Part::Mark, shape, e, col).grow(Grow::Wipe));
        for (i, (x, yy)) in pts.iter().enumerate() {
            let (row, _) = rows[i];
            let tip = c.tip(&[row], &nf);
            let last_point = i == pts.len() - 1;
            let radius = if last_point { r + 1.0 } else { 0.01 };
            let shape = Shape::Circle {
                cx: *x,
                cy: *yy,
                r: radius,
            };
            c.items.push(
                Item::new(Part::Mark, shape, e, col)
                    .datum(row, tip)
                    .highlighted(last_point)
                    .grow(Grow::Wipe),
            );
        }
        c.plot = Rect::from_ltrb(c.left + PAD, y, c.left + c.width - PAD, y + h);
        y += h;
    }
    y + PAD - c.top
}
