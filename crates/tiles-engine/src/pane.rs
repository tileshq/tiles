//! What `tiles pane` reads and draws beyond a card (ADR-063, ADR-065): the
//! hover overlay, two small pictures, a mark's outline and its tooltip,
//! placed above the card so the card itself is never sent again on hover;
//! the legend entries a click toggles; and a grid of text, the
//! stress scene that prices a table before a table form exists. None of
//! them touches how a card is laid out or drawn.

use skia_safe::Rect;

use crate::Options;
use crate::color::Rgba;
use crate::hit::Hit;
use crate::scene::{Anchor, Corners, Emphasis, Item, Part, Scene, Shape};
use crate::text::{Fonts, Style};
use crate::tokens::Tokens;

/// Most tooltip lines drawn.
pub const MAX_TIP_LINES: usize = 6;
/// The tooltip's first line, the mark's name: size and line height.
const TIP_HEAD: (f32, f32) = (12.5, 17.0);
/// Each `name value` line under it.
const TIP_ROW: (f32, f32) = (12.0, 18.0);
/// The hint line at the foot.
const TIP_HINT: (f32, f32) = (11.0, 15.0);
/// Space inside the tooltip's box.
const TIP_PAD: (f32, f32) = (11.0, 9.0);
/// Space above and below the rule under the name.
const TIP_RULE: (f32, f32) = (8.0, 7.0);
/// The swatch before the name, and the space after it.
const TIP_SWATCH: (f32, f32) = (9.0, 7.0);
/// Least space between a name and its value.
const TIP_KV_GAP: f32 = 18.0;
/// Narrowest tooltip.
const TIP_MIN_W: f32 = 168.0;
/// The tooltip's corner radius.
const TIP_RADIUS: f32 = 8.0;
/// Space between the mark's outline and the tooltip.
const TIP_GAP: f32 = 6.0;
/// How far the tooltip's left edge sits left of the pointer.
const TIP_LEAD: f32 = 12.0;
/// How far the outline stands off the mark.
const RING_OUT: f32 = 3.0;
/// The outline's width.
const RING: f32 = 2.0;
/// The card-coloured halo round the outline, each side.
const HALO: f32 = 2.0;
/// The outline's corner radius.
const RING_RADIUS: f32 = 4.0;
/// The tooltip's shadow: drop, spread, opacity.
const SHADOW: (f32, f32, f32) = (4.0, 4.0, 0.3);

/// What the pane knows about a hover beyond the mark: where the pointer
/// is across the card, in CSS pixels from the tile's left, which the
/// tooltip starts just left of; and a hint for its foot, such as what a
/// click would do.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cue<'a> {
    /// The pointer's x in CSS pixels from the tile's left.
    pub pointer_x: Option<f32>,
    /// The hint line.
    pub hint: Option<&'a str>,
}

/// One picture of a hover overlay in device pixels: where it goes on its
/// card, from the card tile's top left, and its scene, ready to draw at
/// the same scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Left edge, in device pixels from the tile's left.
    pub left: i32,
    /// Top edge, in device pixels from the tile's top.
    pub top: i32,
    /// The picture, its own origin at (`left`, `top`) over the tile.
    pub scene: Scene,
}

/// A hover overlay: the mark's ring and the tooltip as two tight
/// pictures, so the space between them is neither drawn nor sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    /// The ring round the mark.
    pub ring: Piece,
    /// The tooltip.
    pub tip: Piece,
}

/// `items`, drawn in CSS pixels over `card`, as one picture covering
/// `bounds` cut to the tile, at device `scale`.
#[allow(clippy::cast_possible_truncation)] // Card coordinates are far below i32::MAX.
fn piece(card: &Scene, bounds: Rect, scale: f32, items: Vec<Item>) -> Piece {
    let tile = Rect::from_wh(card.width, card.height);
    let mut px = Rect::from_ltrb(
        (bounds.left.max(tile.left) * scale).floor(),
        (bounds.top.max(tile.top) * scale).floor(),
        (bounds.right.min(tile.right) * scale).ceil(),
        (bounds.bottom.min(tile.bottom) * scale).ceil(),
    );
    if px.width() < 1.0 || px.height() < 1.0 {
        px = Rect::from_xywh(px.left, px.top, 1.0, 1.0);
    }
    let (dx, dy) = (px.left / scale, px.top / scale);
    let (width, height) = (px.width() / scale, px.height() / scale);
    Piece {
        left: px.left as i32,
        top: px.top as i32,
        scene: Scene {
            form: "overlay".into(),
            theme: card.theme,
            width,
            height,
            card: Rect::from_wh(width, height),
            boxed: false,
            plot: Rect::from_wh(width, height),
            y_axis: false,
            items: items.into_iter().map(|i| shifted(i, -dx, -dy)).collect(),
        },
    }
}

#[allow(clippy::cast_precision_loss)] // Pixel coordinates far below 2^24.
fn css(px: i32, scale: f32) -> f32 {
    px as f32 / scale
}

/// One tooltip as laid out: the mark's name, its `name value` lines and
/// the hint, with the box they need.
struct Tip<'a> {
    head: &'a str,
    rows: Vec<(&'a str, &'a str)>,
    hint: Option<&'a str>,
    w: f32,
    h: f32,
}

fn head_style() -> Style {
    Style::new(TIP_HEAD.0, 600)
}

fn key_style() -> Style {
    Style::new(TIP_ROW.0, 400)
}

fn value_style() -> Style {
    Style::new(TIP_ROW.0, 600).tabular()
}

fn hint_style() -> Style {
    Style::new(TIP_HINT.0, 400)
}

/// The tooltip for a hit's `name: value` lines: the first line's value
/// names the mark, the rest stand as name and value.
fn tip<'a>(fonts: &Fonts, text: &'a str, hint: Option<&'a str>, card_w: f32) -> Tip<'a> {
    let split = |l: &'a str| l.split_once(": ").unwrap_or((l, ""));
    let mut lines = text.lines().take(MAX_TIP_LINES);
    let head = lines.next().map_or("", |l| {
        let (k, v) = split(l);
        if v.is_empty() { k } else { v }
    });
    let rows: Vec<(&str, &str)> = lines.map(split).collect();
    let head_w = TIP_SWATCH.0 + TIP_SWATCH.1 + fonts.width(head, head_style());
    let row_w = rows
        .iter()
        .map(|(k, v)| fonts.width(k, key_style()) + TIP_KV_GAP + fonts.width(v, value_style()))
        .fold(0.0, f32::max);
    let hint_w = hint.map_or(0.0, |h| fonts.width(h, hint_style()));
    let w = (head_w.max(row_w).max(hint_w) + 2.0 * TIP_PAD.0)
        .max(TIP_MIN_W)
        .min(card_w);
    let mut h = 2.0 * TIP_PAD.1 + TIP_HEAD.1;
    if !rows.is_empty() {
        #[allow(clippy::cast_precision_loss)] // At most MAX_TIP_LINES.
        let n = rows.len() as f32;
        h += TIP_RULE.0 + 1.0 + TIP_RULE.1 + n * TIP_ROW.1;
    }
    if hint.is_some() {
        h += 6.0 + TIP_HINT.1;
    }
    Tip {
        head,
        rows,
        hint,
        w,
        h,
    }
}

/// The colour the card draws row `row` in, for the tooltip's swatch.
fn mark_colour(card: &Scene, row: Option<usize>) -> Option<Rgba> {
    let row = row?;
    card.items
        .iter()
        .find(|i| i.part == Part::Mark && i.datum == Some(row))
        .map(|i| i.color)
}

/// The boxes a tooltip must leave clear: the card's title and the line
/// under it, and every value label, each with a little room round it.
fn keep_clear(fonts: &Fonts, card: &Scene) -> Vec<Rect> {
    card.items
        .iter()
        .filter(|i| matches!(i.part, Part::Title | Part::Subtitle | Part::Value))
        .filter_map(|i| match &i.shape {
            Shape::Text {
                text,
                x,
                y,
                style,
                anchor,
            } => {
                let w = fonts.width(text, *style);
                let left = match anchor {
                    Anchor::Start => *x,
                    Anchor::Middle => x - w / 2.0,
                    Anchor::End => x - w,
                };
                Some(
                    Rect::from_ltrb(left, y - style.size, left + w, y + 0.3 * style.size)
                        .with_outset((2.0, 2.0)),
                )
            }
            _ => None,
        })
        .collect()
}

/// The box a tooltip at `left`, `top` covers with its hairline and shadow.
fn footprint(left: f32, top: f32, w: f32, h: f32) -> Rect {
    Rect::from_ltrb(
        left - SHADOW.1,
        top - 1.0,
        left + w + SHADOW.1,
        top + h + SHADOW.0 + SHADOW.1,
    )
}

/// The place in `lo..=hi` nearest `pref`, a pixel at a time, where `free`
/// holds.
fn nearest(lo: f32, hi: f32, pref: f32, free: impl Fn(f32) -> bool) -> Option<f32> {
    if hi < lo {
        return None;
    }
    let pref = pref.clamp(lo, hi);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // A tile's span.
    let steps = (hi - lo).ceil() as usize;
    (0..=steps).find_map(|d| {
        #[allow(clippy::cast_precision_loss)] // At most a tile's span.
        let d = d as f32;
        [pref - d, pref + d]
            .into_iter()
            .find(|at| (lo..=hi).contains(at) && free(*at))
    })
}

/// The clear place for a `w` by `h` tooltip anywhere in `card` nearest
/// `pref`, or `None` when the boxes in `clear` leave none. If any place is
/// clear, one is whose left and top each sit at the tile's edge or just
/// past a box's edge (slide a clear tooltip left, then up, until it
/// stops), so only those are tried.
fn anywhere(
    clear: &[Rect],
    card: &Scene,
    (w, h): (f32, f32),
    pref: (f32, f32),
    free: impl Fn(f32, f32) -> bool,
) -> Option<(f32, f32)> {
    let (max_left, max_top) = (card.width - w, card.height - h);
    let lefts = clear
        .iter()
        .flat_map(|c| [c.right + SHADOW.1, c.left - w - SHADOW.1])
        .chain([0.0, max_left, pref.0.clamp(0.0, max_left.max(0.0))]);
    let tops: Vec<f32> = clear
        .iter()
        .flat_map(|c| [c.bottom + 1.0, c.top - h - SHADOW.0 - SHADOW.1])
        .chain([0.0, max_top, pref.1.clamp(0.0, max_top.max(0.0))])
        .collect();
    lefts
        .flat_map(|l| tops.iter().map(move |&t| (l, t)))
        .filter(|&(l, t)| (0.0..=max_left).contains(&l) && (0.0..=max_top).contains(&t))
        .filter(|&(l, t)| free(l, t))
        .min_by(|a, b| {
            let d = |(l, t): (f32, f32)| (l - pref.0).powi(2) + (t - pref.1).powi(2);
            d(*a).total_cmp(&d(*b))
        })
}

/// Where a `w` by `h` tooltip for the mark `mark`, outlined by `ring`,
/// goes on `card` (ADR-070): below the mark, from `pref_left`; else beside
/// it, on the side with more room, inside the plot's width, from the plot's
/// top down to the axis labels under it; else inside the mark's
/// upper part when the mark is wide and tall enough; else the clear place
/// in the tile nearest `pref_left` just below the mark; each only where it
/// leaves the card's title and value labels clear. When none does, below
/// the mark, or above it when the tile has no room below.
fn place_tip(
    fonts: &Fonts,
    card: &Scene,
    (mark, ring): (Rect, Rect),
    (w, h): (f32, f32),
    pref_left: f32,
) -> (f32, f32) {
    let clear = keep_clear(fonts, card);
    let free = |left: f32, top: f32| {
        let f = footprint(left, top, w, h);
        clear.iter().all(|c| {
            f.right <= c.left || c.right <= f.left || f.bottom <= c.top || c.bottom <= f.top
        })
    };
    let reach = RING / 2.0 + HALO + TIP_GAP;
    let below = ring.bottom + reach;
    if below + h <= card.height
        && let Some(left) = nearest(0.0, card.width - w, pref_left, |l| free(l, below))
    {
        return (left, below);
    }
    let plot = card.plot;
    let right = ring.right + reach;
    let left = ring.left - reach - w;
    let mut sides = [
        (plot.right - ring.right, right, right + w <= plot.right),
        (ring.left - plot.left, left, left >= plot.left),
    ];
    if sides[1].0 > sides[0].0 {
        sides.swap(0, 1);
    }
    for (_, left, fits) in sides {
        if fits
            && let Some(top) = nearest(plot.top, card.height - h, ring.top, |t| free(left, t))
        {
            return (left, top);
        }
    }
    let inside = (mark.center_x() - w / 2.0, mark.top + TIP_GAP);
    if mark.width() >= w + 2.0 * TIP_GAP
        && inside.1 + h <= mark.bottom
        && free(inside.0, inside.1)
    {
        return inside;
    }
    if let Some(at) = anywhere(&clear, card, (w, h), (pref_left, below), &free) {
        return at;
    }
    let top = if below + h <= card.height {
        below
    } else {
        (ring.top - reach - h).max(0.0)
    };
    (pref_left.min(card.width - w).max(0.0), top)
}

/// The overlay for `hit` on `card`, at device `scale`: the mark outlined
/// in ink inside a card-coloured halo, and, as a picture of its own, a
/// raised tooltip placed by [`place_tip`], starting just left of the
/// pointer when it sits below the mark, kept inside the tile (ADR-070).
pub(crate) fn overlay(
    tokens: &Tokens,
    fonts: &Fonts,
    card: &Scene,
    hit: &Hit,
    scale: f32,
    cue: Cue<'_>,
) -> Overlay {
    let pal = tokens.palette(card.theme);
    let mark = Rect::from_ltrb(
        css(hit.left, scale),
        css(hit.top, scale),
        css(hit.right, scale),
        css(hit.bottom, scale),
    );
    let ring = mark.with_outset((RING_OUT, RING_OUT));
    let laid = tip(fonts, &hit.tip, cue.hint, card.width);
    let reach = RING / 2.0 + HALO;
    let pref_left = cue
        .pointer_x
        .map_or(ring.center_x() - laid.w / 2.0, |px| px - TIP_LEAD);
    let (left, top) = place_tip(fonts, card, (mark, ring), (laid.w, laid.h), pref_left);
    let tip_box = Rect::from_xywh(left, top, laid.w, laid.h);
    let shadow = tip_box
        .with_offset((0.0, SHADOW.0))
        .with_outset((SHADOW.1, SHADOW.1));
    let path = rounded(ring, RING_RADIUS);
    let ring_piece = piece(
        card,
        ring.with_outset((reach, reach)),
        scale,
        vec![
            line(path.clone(), RING + 2.0 * HALO, pal.card),
            line(path, RING, pal.ink),
        ],
    );
    let mut items = vec![
        rect(shadow, TIP_RADIUS + SHADOW.1, Rgba::black(SHADOW.2)),
        rect(
            tip_box.with_outset((1.0, 1.0)),
            TIP_RADIUS + 1.0,
            pal.raised_line(),
        ),
        rect(tip_box, TIP_RADIUS, pal.raised()),
    ];
    items.extend(tip_text(
        fonts,
        pal,
        &laid,
        tip_box,
        mark_colour(card, hit.row),
    ));
    let bounds = Rect::from_ltrb(shadow.left, tip_box.top - 1.0, shadow.right, shadow.bottom);
    Overlay {
        ring: ring_piece,
        tip: piece(card, bounds, scale, items),
    }
}

/// The tooltip's swatch and text inside `tip_box`: the mark's name after
/// a swatch of its colour, a rule, each name and value, and the hint.
fn tip_text(
    fonts: &Fonts,
    pal: &crate::tokens::Palette,
    laid: &Tip<'_>,
    tip_box: Rect,
    swatch: Option<Rgba>,
) -> Vec<Item> {
    let mut items = Vec::new();
    let left = tip_box.left + TIP_PAD.0;
    let mut at = tip_box.top + TIP_PAD.1;
    let swatch = swatch.unwrap_or(pal.grey);
    let sw_top = at + (TIP_HEAD.1 - TIP_SWATCH.0) / 2.0;
    items.push(rect(
        Rect::from_xywh(left, sw_top, TIP_SWATCH.0, TIP_SWATCH.0),
        2.0,
        swatch,
    ));
    let text_w = tip_box.width() - 2.0 * TIP_PAD.0;
    items.push(text(
        fit(
            fonts,
            laid.head,
            head_style(),
            text_w - TIP_SWATCH.0 - TIP_SWATCH.1,
        ),
        left + TIP_SWATCH.0 + TIP_SWATCH.1,
        at + fonts.baseline_in_line(TIP_HEAD.0, TIP_HEAD.1),
        head_style(),
        Anchor::Start,
        (Emphasis::Ink, pal.ink),
    ));
    at += TIP_HEAD.1;
    if !laid.rows.is_empty() {
        at += TIP_RULE.0;
        items.push(rect(
            Rect::from_xywh(tip_box.left, at, tip_box.width(), 1.0),
            0.0,
            pal.raised_line(),
        ));
        at += 1.0 + TIP_RULE.1;
        for (k, v) in &laid.rows {
            let baseline = at + fonts.baseline_in_line(TIP_ROW.0, TIP_ROW.1);
            let vw = fonts.width(v, value_style()).min(text_w);
            items.push(text(
                fit(fonts, k, key_style(), (text_w - vw - TIP_KV_GAP).max(0.0)),
                left,
                baseline,
                key_style(),
                Anchor::Start,
                (Emphasis::Muted, pal.muted),
            ));
            items.push(text(
                fit(fonts, v, value_style(), text_w),
                tip_box.right - TIP_PAD.0,
                baseline,
                value_style(),
                Anchor::End,
                (Emphasis::Ink, pal.ink),
            ));
            at += TIP_ROW.1;
        }
    }
    if let Some(h) = laid.hint {
        at += 6.0;
        items.push(text(
            fit(fonts, h, hint_style(), text_w),
            left,
            at + fonts.baseline_in_line(TIP_HINT.0, TIP_HINT.1),
            hint_style(),
            Anchor::Start,
            (Emphasis::Muted, pal.muted.with_opacity(0.75)),
        ));
    }
    items
}

/// A closed outline of `r` with corners rounded by `radius`, as points.
fn rounded(r: Rect, radius: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 6;
    let radius = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    let corners = [
        (r.right - radius, r.top + radius, -90.0f32),
        (r.right - radius, r.bottom - radius, 0.0),
        (r.left + radius, r.bottom - radius, 90.0),
        (r.left + radius, r.top + radius, 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1) + 1);
    for (cx, cy, start) in corners {
        for s in 0..=STEPS {
            #[allow(clippy::cast_precision_loss)] // A handful of steps.
            let a = (start + 90.0 * s as f32 / STEPS as f32).to_radians();
            points.push((cx + radius * a.cos(), cy + radius * a.sin()));
        }
    }
    if let Some(&first) = points.first() {
        points.push(first);
    }
    points
}

fn line(points: Vec<(f32, f32)>, width: f32, color: Rgba) -> Item {
    Item::new(
        Part::Guide,
        Shape::Line { points, width },
        Emphasis::Ink,
        color,
    )
}

fn text(
    text: String,
    x: f32,
    y: f32,
    style: Style,
    anchor: Anchor,
    (emphasis, color): (Emphasis, Rgba),
) -> Item {
    Item::new(
        Part::Value,
        Shape::Text {
            text,
            x,
            y,
            style,
            anchor,
        },
        emphasis,
        color,
    )
}

/// The selection a click on a mark made, for the pane's heading: the
/// column and the value (ADR-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chip<'a> {
    /// The column, such as `region`.
    pub column: &'a str,
    /// The value, such as `France`.
    pub value: &'a str,
}

/// The chip's height and corner radius.
const CHIP_H: f32 = 24.0;
const CHIP_RADIUS: f32 = 7.0;
/// Space inside the chip at each end, and between its parts.
const CHIP_PAD: f32 = 10.0;
const CHIP_GAP: f32 = 7.0;
/// The chip's square dot.
const CHIP_DOT: f32 = 8.0;
/// The chip's text size.
const CHIP_SIZE: f32 = 12.5;
/// The key cap after the chip: height, padding, text size.
const KEY_CAP: (f32, f32, f32) = (18.0, 5.0, 11.5);
/// Space between the chip and the key cap, and after the key cap.
const CHIP_TAIL: (f32, f32) = (14.0, 6.0);
/// What the key cap and the words after it say.
const RESET_KEY: (&str, &str) = ("r", "resets");
/// The least share of the heading's width the title keeps beside the
/// chip; below it the chip takes its own line.
const INLINE_TITLE: f32 = 0.6;
/// Space between the title and a chip beside it.
const CHIP_CLEAR: f32 = 24.0;
/// Space above and below a chip on its own line.
const CHIP_LINE_PAD: f32 = 6.0;

/// The pane's heading: the dashboard's title and note as a card heading
/// draws them, and, when a click selected a value, its chip, `column
/// value ×`, then the key that clears it. Where the title keeps at least
/// [`INLINE_TITLE`] of the width beside the widest chip, the chip sits at
/// the right of the title line and the title wraps short of it; on a
/// narrower pane the chip takes its own line under the title, which keeps
/// the full width. Either way the heading leaves the chip's room whether
/// a value is focused or not, so focusing one never reflows it. The note
/// takes the full width.
pub(crate) fn heading(
    tokens: &Tokens,
    fonts: &Fonts,
    title: &str,
    note: Option<&str>,
    chip: Option<Chip<'_>>,
    opts: &Options,
) -> Scene {
    let pal = tokens.palette(opts.theme);
    let key = Style::new(CHIP_SIZE, 400);
    let value = Style::new(CHIP_SIZE, 500);
    let cap = Style::new(KEY_CAP.2, 500);
    let words = Style::new(CHIP_SIZE, 400);
    let frame = 2.0 * CHIP_PAD + CHIP_DOT + 3.0 * CHIP_GAP + fonts.width("×", value);
    let cap_w = fonts.width(RESET_KEY.0, cap) + 2.0 * KEY_CAP.1;
    let tail_w = CHIP_TAIL.0 + cap_w + CHIP_TAIL.1 + fonts.width(RESET_KEY.1, words);
    let side = tokens.card.margin_x + tokens.card.border;
    let inner = opts.width - 2.0 * side;
    let caps = (opts.width / 12.0, opts.width / 7.0);
    let beside = inner - frame - caps.0 - caps.1 - tail_w - CHIP_CLEAR;
    let inline = beside >= INLINE_TITLE * inner;
    let ((column_max, value_max), room) = if inline {
        (caps, (beside, 0.0))
    } else {
        let free = inner - frame - tail_w;
        (
            (0.4 * free, 0.6 * free),
            (inner, CHIP_H + 2.0 * CHIP_LINE_PAD),
        )
    };
    let mut scene = crate::layout::heading_in(tokens, fonts, title, note, room, opts);
    let Some(chip) = chip else {
        return scene;
    };
    let column = fit(fonts, chip.column, key, column_max);
    let value_text = fit(fonts, chip.value, value, value_max);
    let chip_w = frame + fonts.width(&column, key) + fonts.width(&value_text, value);
    let title_line = tokens.text.title_line * 1.2;
    let (x, mid) = if inline {
        (opts.width - side, tokens.card.margin_top + title_line / 2.0)
    } else {
        let lines = f32::from(u16::try_from(scene.part(Part::Title).count()).unwrap_or(u16::MAX));
        (
            side + chip_w + tail_w,
            tokens.card.margin_top + lines * title_line + CHIP_LINE_PAD + CHIP_H / 2.0,
        )
    };
    let mut row = Leftward {
        fonts,
        x,
        mid,
        items: std::mem::take(&mut scene.items),
    };
    row.put(RESET_KEY.1, words, (Emphasis::Muted, pal.muted));
    row.x -= CHIP_TAIL.1;
    let cap_box = Rect::from_xywh(row.x - cap_w, mid - KEY_CAP.0 / 2.0, cap_w, KEY_CAP.0);
    row.items.push(rect(cap_box, 4.0, pal.grid));
    row.x -= KEY_CAP.1;
    row.put(RESET_KEY.0, cap, (Emphasis::Ink, pal.ink));
    row.x -= KEY_CAP.1 + CHIP_TAIL.0;
    let chip_box = Rect::from_xywh(row.x - chip_w, mid - CHIP_H / 2.0, chip_w, CHIP_H);
    let soft = (Emphasis::Accent, pal.accent.mix(pal.ink, 0.45));
    let page = pal.page();
    row.items
        .push(rect(chip_box, CHIP_RADIUS, page.mix(pal.accent, 0.45)));
    row.items.push(rect(
        chip_box.with_inset((1.0, 1.0)),
        CHIP_RADIUS - 1.0,
        page.mix(pal.accent, 0.16),
    ));
    row.x -= CHIP_PAD;
    row.put("×", value, soft);
    row.x -= CHIP_GAP;
    row.put(
        &value_text,
        value,
        (Emphasis::Accent, pal.accent.mix(pal.ink, 0.8)),
    );
    row.x -= CHIP_GAP;
    row.put(&column, key, soft);
    row.x -= CHIP_GAP + CHIP_DOT;
    let dot = Rect::from_xywh(row.x, mid - CHIP_DOT / 2.0, CHIP_DOT, CHIP_DOT);
    row.items.push(rect(dot, 2.0, pal.accent));
    scene.items = row.items;
    scene
}

/// Text set right to left along a line centred on `mid`, from `x`.
struct Leftward<'a> {
    fonts: &'a Fonts,
    x: f32,
    mid: f32,
    items: Vec<Item>,
}

impl Leftward<'_> {
    /// Sets `s` ending at `x` and moves `x` past it.
    fn put(&mut self, s: &str, style: Style, ink: (Emphasis, Rgba)) {
        self.x -= self.fonts.width(s, style);
        self.items.push(text(
            s.to_string(),
            self.x,
            self.mid + style.size * 0.36,
            style,
            Anchor::Start,
            ink,
        ));
    }
}

fn rect(r: Rect, radius: f32, color: Rgba) -> Item {
    Item::new(
        Part::Guide,
        Shape::Rect {
            rect: r,
            radius,
            corners: Corners::All,
        },
        Emphasis::Ink,
        color,
    )
}

/// `item` moved by `dx`, `dy`; only rectangles and text occur here.
fn shifted(mut item: Item, dx: f32, dy: f32) -> Item {
    match &mut item.shape {
        Shape::Rect { rect, .. } => *rect = rect.with_offset((dx, dy)),
        Shape::Text { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        Shape::Circle { cx, cy, .. } => {
            *cx += dx;
            *cy += dy;
        }
        Shape::Line { points, .. } | Shape::Area { points, .. } => {
            for p in points {
                p.0 += dx;
                p.1 += dy;
            }
        }
    }
    item
}

/// One legend entry a click can toggle: its swatch and label in device
/// pixels from the card's top left, and the label as drawn, which a long
/// name ends with an ellipsis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendHit {
    /// Left edge in device pixels.
    pub left: i32,
    /// Top edge in device pixels.
    pub top: i32,
    /// Right edge in device pixels.
    pub right: i32,
    /// Bottom edge in device pixels.
    pub bottom: i32,
    /// The label as drawn.
    pub label: String,
}

impl LegendHit {
    /// Whether a device pixel falls inside.
    #[must_use]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// The legend entries of `card` at device `scale`: each legend label's box,
/// widened to the swatch drawn just before it. A line chart's direct labels
/// count, since they are its legend.
#[allow(clippy::cast_possible_truncation)] // Card coordinates are far below i32::MAX.
pub(crate) fn legend(fonts: &Fonts, card: &Scene, scale: f32) -> Vec<LegendHit> {
    let mut out = Vec::new();
    let mut swatch: Option<Rect> = None;
    for item in &card.items {
        if item.part != Part::Legend {
            swatch = None;
            continue;
        }
        let Shape::Text {
            text,
            x,
            y,
            style,
            anchor,
        } = &item.shape
        else {
            swatch = Some(item.bounds());
            continue;
        };
        let w = fonts.width(text, *style);
        let left = match anchor {
            Anchor::Start => *x,
            Anchor::Middle => x - w / 2.0,
            Anchor::End => x - w,
        };
        let mut b = Rect::from_ltrb(left, y - style.size, left + w, y + style.size * 0.3);
        if let Some(s) = swatch.take() {
            b = Rect::from_ltrb(
                b.left.min(s.left),
                b.top.min(s.top),
                b.right.max(s.right),
                b.bottom.max(s.bottom),
            );
        }
        out.push(LegendHit {
            left: (b.left * scale).floor() as i32,
            top: (b.top * scale).floor() as i32,
            right: (b.right * scale).ceil() as i32,
            bottom: (b.bottom * scale).ceil() as i32,
            label: text.clone(),
        });
    }
    out
}

/// Space between a grid's rows, in CSS pixels.
const GRID_ROW: f32 = 20.0;
/// A grid's text size.
const GRID_SIZE: f32 = 12.0;

/// A card holding `header` over `rows` of text in equal columns, each cell
/// cut with an ellipsis to its column: the table stress scene.
pub(crate) fn text_grid(
    tokens: &Tokens,
    fonts: &Fonts,
    header: &[String],
    rows: &[Vec<String>],
    opts: &Options,
) -> Scene {
    let t = &tokens.card;
    let pal = tokens.palette(opts.theme);
    let inset = t.margin_x + t.border + t.padding;
    let width = opts.width;
    let inner = (width - 2.0 * inset).max(1.0);
    #[allow(clippy::cast_precision_loss)] // A handful of columns.
    let col_w = inner / header.len().max(1) as f32;
    let mut items = Vec::new();
    let top = t.margin_top + t.border + t.padding;
    let line = |rows_down: f32| top + rows_down * GRID_ROW;
    let mut push_row = |cells: &[String], y: f32, style: Style, emphasis: Emphasis, color: Rgba| {
        for (c, cell) in cells.iter().take(header.len()).enumerate() {
            #[allow(clippy::cast_precision_loss)] // A handful of columns.
            let x = inset + c as f32 * col_w;
            items.push(Item::new(
                Part::Value,
                Shape::Text {
                    text: fit(fonts, cell, style, col_w - 8.0),
                    x,
                    y,
                    style,
                    anchor: Anchor::Start,
                },
                emphasis,
                color,
            ));
        }
    };
    let baseline = fonts.baseline_in_line(GRID_SIZE, GRID_ROW);
    push_row(
        header,
        line(0.0) + baseline,
        Style::new(GRID_SIZE, 600),
        Emphasis::Muted,
        pal.muted,
    );
    for (r, row) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // Tens of rows.
        let y = line(1.0 + r as f32) + baseline;
        push_row(
            row,
            y,
            Style::new(GRID_SIZE, 400).tabular(),
            Emphasis::Ink,
            pal.ink,
        );
    }
    items.push(Item::new(
        Part::Grid,
        Shape::Rect {
            rect: Rect::from_xywh(inset, line(1.0) - 1.0, inner, 1.0),
            radius: 0.0,
            corners: Corners::All,
        },
        Emphasis::Grid,
        pal.grid,
    ));
    #[allow(clippy::cast_precision_loss)] // Tens of rows.
    let bottom = line(1.0 + rows.len() as f32) + t.padding + t.border;
    let mut height = bottom + t.margin_bottom;
    if let Some(cell) = opts.cell_height.filter(|c| *c > 0.0) {
        height = (height / cell - 0.01).ceil() * cell;
    }
    let card = Rect::from_ltrb(t.margin_x, t.margin_top, width - t.margin_x, bottom);
    Scene {
        form: "text-grid".into(),
        theme: opts.theme,
        width,
        height,
        card,
        boxed: true,
        plot: card.with_inset((t.padding, t.padding)),
        y_axis: false,
        items,
    }
}

/// `text` cut with an ellipsis to `max` CSS pixels.
pub(crate) fn fit(fonts: &Fonts, text: &str, style: Style, max: f32) -> String {
    if fonts.width(text, style) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().chain(std::iter::once(&'…')).collect();
        if fonts.width(&cut, style) <= max {
            return cut;
        }
    }
    String::new()
}
