//! The funnel: how many reach each step, against the step before.

use skia_safe::Rect;

use super::{Ctx, LABEL_SHARE, MIDDLE, PAD, f};
use crate::format::NumberFormat;
use crate::scale::{Band, Linear};
use crate::scene::{Anchor, Corners, Emphasis, Grow, Item, Part, Shape};

/// The funnel's plot height, as the gallery's funnel sets it.
const HEIGHT: f32 = 210.0;

/// Most room one step takes, so a short funnel stays a row of bars.
const MAX_STEP: f32 = 48.0;

/// A share of the step before as written: whole percent, and `<1` or `>99`
/// where it only rounds to 0 or 100, as shares read (playbook M12).
fn share_text(s: f64) -> String {
    if s > 0.0 && s < 0.5 {
        "<1%".into()
    } else if (99.5..100.0).contains(&s) {
        ">99%".into()
    } else {
        format!("{}%", NumberFormat::for_values(&[0.0]).format(s.round()))
    }
}

/// Each step's bar over a ghost of the step before, from zero, in the
/// order the steps are sent; each bar carries its value and its share of
/// the step before (playbook M1), so the funnel has no axis.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names
)] // A layout reads top to bottom as geometry: x, y, h are scales and sizes.
pub(super) fn funnel(c: &mut Ctx) -> f32 {
    let pts = c.categories("y");
    let values: Vec<f64> = pts.iter().map(|p| p.value).collect();
    let nf = NumberFormat::for_values(&values).with_unit(c.intent.unit);
    let ls = c.label_style();
    let vs = c.row_value_style();
    let off = c.t.marks.row_value_offset;
    let names: Vec<String> = pts.iter().map(|p| p.label.clone()).collect();
    let shown = c.fit_all(&names, ls, LABEL_SHARE);
    let widest = shown.iter().map(|l| c.width_of(l, ls)).fold(0.0, f32::max);
    // Each step's share of the one before, where the one before has some.
    let shares: Vec<Option<f64>> = (0..pts.len())
        .map(|i| {
            let prev = i.checked_sub(1).map(|p| values[p])?;
            (prev > 0.0).then(|| values[i] / prev * 100.0)
        })
        .collect();
    let labels: Vec<String> = values
        .iter()
        .zip(&shares)
        .map(|(v, s)| match s {
            Some(s) => format!("{} ({})", nf.format(*v), share_text(*s)),
            None => nf.format(*v),
        })
        .collect();
    let left = c.left + PAD + widest + c.t.marks.label_padding;
    let limit = c.left + c.width - PAD;
    let max = values.iter().copied().fold(0.0, f64::max);
    let hi = if max > 0.0 { max } else { 1.0 };
    // The scale runs as far as the longest label past its bar leaves room.
    let right = values
        .iter()
        .zip(&labels)
        .fold(limit, |right, (v, l)| {
            #[allow(clippy::cast_possible_truncation)] // A share of the scale, 0 to 1.
            let t = (v / hi) as f32;
            let room = limit - left - off - c.width_of(l, vs);
            if t > 0.0 {
                right.min(left + room / t)
            } else {
                right
            }
        })
        .max(left + 1.0);
    let x = Linear {
        d0: 0.0,
        d1: hi,
        r0: left,
        r1: right,
    };
    let inner = c.t.marks.band_padding_inner;
    let outer = c.t.marks.band_padding_outer;
    let span_of = |step: f32| step * (f(pts.len()) - inner + 2.0 * outer);
    let step = (c.t.marks.row_step + 4.0).max((HEIGHT / span_of(1.0)).min(MAX_STEP));
    let top = c.top + PAD;
    let span = span_of(step);
    let band = Band::new(pts.len(), top, top + span, inner, outer);
    let thick = band.width * c.t.marks.bar_band;
    for (i, p) in pts.iter().enumerate() {
        let cy = band.center(i);
        let hl = c.is_highlight(&p.label);
        c.text(
            Part::AxisLabel,
            shown[i].clone(),
            c.left + PAD + widest,
            cy + (MIDDLE * ls.size).round(),
            ls,
            Anchor::End,
            (Emphasis::Muted, c.pal.muted),
        );
        let (y0, y1) = (cy - thick / 2.0, cy + thick / 2.0);
        if i > 0 {
            let ghost = Rect::from_ltrb(left, y0, x.map(values[i - 1].max(0.0)), y1);
            let shape = Shape::Rect {
                rect: ghost,
                radius: c.t.marks.bar_radius,
                corners: Corners::Right,
            };
            c.items
                .push(Item::new(Part::Guide, shape, Emphasis::Track, c.pal.track));
        }
        let (e, col) = c.mark_color(hl);
        let end = x.map(p.value.max(0.0));
        let shape = Shape::Rect {
            rect: Rect::from_ltrb(left, y0, end, y1),
            radius: c.t.marks.bar_radius,
            corners: Corners::Right,
        };
        let mut tip = c.tip(&p.rows, &nf);
        if let Some(s) = shares[i] {
            tip = format!("{tip}\nof the step before: {}", share_text(s));
        }
        c.items.push(
            Item::new(Part::Mark, shape, e, col)
                .datum(p.rows[0], tip)
                .highlighted(hl)
                .grow(Grow::FromX(left)),
        );
        let at = c.items.len();
        let vc = c.value_color(hl);
        c.text(
            Part::Value,
            labels[i].clone(),
            end + off,
            cy + (MIDDLE * vs.size).round(),
            vs,
            Anchor::Start,
            vc,
        );
        c.items[at].grow = Grow::Fade;
    }
    c.plot = Rect::from_ltrb(left, top, right, top + span);
    span + 2.0 * PAD
}

#[cfg(test)]
mod tests {
    use super::share_text;

    #[test]
    fn a_share_that_only_rounds_to_an_end_says_so() {
        assert_eq!(share_text(62.0), "62%");
        assert_eq!(share_text(0.0), "0%");
        assert_eq!(share_text(0.12), "<1%");
        assert_eq!(share_text(99.7), ">99%");
        assert_eq!(share_text(100.0), "100%");
        assert_eq!(share_text(120.0), "120%");
    }
}
