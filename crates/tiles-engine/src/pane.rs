//! What `tiles pane` reads and draws beyond a card (ADR-063, ADR-065): the
//! hover overlay, one small picture holding a mark's outline and its
//! tooltip, placed above the card so the card itself is never sent again on
//! hover; the legend entries a click toggles; and a grid of text, the
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
/// Tooltip text size, in CSS pixels.
const TIP_SIZE: f32 = 12.0;
/// Tooltip line height.
const TIP_LINE: f32 = 17.0;
/// Space inside the tooltip's box.
const TIP_PAD: (f32, f32) = (9.0, 6.0);
/// Space between the mark's outline and the tooltip.
const TIP_GAP: f32 = 6.0;
/// How far the outline stands off the mark.
const RING_OUT: f32 = 3.0;
/// The outline's width.
const RING: f32 = 2.0;

/// A hover overlay in device pixels: where it goes on its card, from the
/// card tile's top left, and its scene, ready to draw at the same scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    /// Left edge, in device pixels from the tile's left.
    pub left: i32,
    /// Top edge, in device pixels from the tile's top.
    pub top: i32,
    /// The overlay, its own origin at (`left`, `top`) over the tile.
    pub scene: Scene,
}

#[allow(clippy::cast_precision_loss)] // Pixel coordinates far below 2^24.
fn css(px: i32, scale: f32) -> f32 {
    px as f32 / scale
}

/// The overlay for `hit` on `card`, at device `scale`: the mark outlined
/// in ink and its tooltip above it, or below when there is no room above,
/// kept inside the tile.
#[allow(clippy::cast_possible_truncation)] // Card coordinates are far below i32::MAX.
pub(crate) fn overlay(
    tokens: &Tokens,
    fonts: &Fonts,
    card: &Scene,
    hit: &Hit,
    scale: f32,
) -> Overlay {
    let pal = tokens.palette(card.theme);
    let ring = Rect::from_ltrb(
        css(hit.left, scale),
        css(hit.top, scale),
        css(hit.right, scale),
        css(hit.bottom, scale),
    )
    .with_outset((RING_OUT, RING_OUT));
    let lines: Vec<(&str, Style)> = hit
        .tip
        .lines()
        .take(MAX_TIP_LINES)
        .enumerate()
        .map(|(i, l)| {
            (
                l,
                Style::new(TIP_SIZE, if i == 0 { 600 } else { 400 }).tabular(),
            )
        })
        .collect();
    let tip = tip_box(fonts, ring, &lines, card);
    let bounds = Rect::from_ltrb(
        ring.left.min(tip.left) - RING,
        ring.top.min(tip.top) - RING,
        ring.right.max(tip.right) + RING,
        ring.bottom.max(tip.bottom) + RING,
    );
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
    let mut items = Vec::new();
    for side in [
        Rect::from_ltrb(
            ring.left - RING,
            ring.top - RING,
            ring.right + RING,
            ring.top,
        ),
        Rect::from_ltrb(
            ring.left - RING,
            ring.bottom,
            ring.right + RING,
            ring.bottom + RING,
        ),
        Rect::from_ltrb(ring.left - RING, ring.top, ring.left, ring.bottom),
        Rect::from_ltrb(ring.right, ring.top, ring.right + RING, ring.bottom),
    ] {
        items.push(rect(side, 0.0, pal.ink));
    }
    let radius = tokens.card.radius / 2.0;
    items.push(rect(tip.with_outset((1.0, 1.0)), radius + 1.0, pal.line));
    items.push(rect(tip, radius, pal.card));
    for (i, (line, style)) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // At most MAX_TIP_LINES.
        let top = tip.top + TIP_PAD.1 + i as f32 * TIP_LINE;
        let (emphasis, color) = if i == 0 {
            (Emphasis::Ink, pal.ink)
        } else {
            (Emphasis::Muted, pal.muted)
        };
        items.push(Item::new(
            Part::Value,
            Shape::Text {
                text: (*line).to_string(),
                x: tip.left + TIP_PAD.0,
                y: top + fonts.baseline_in_line(TIP_SIZE, TIP_LINE),
                style: *style,
                anchor: Anchor::Start,
            },
            emphasis,
            color,
        ));
    }
    let (dx, dy) = (px.left / scale, px.top / scale);
    let (w, h) = (px.width() / scale, px.height() / scale);
    Overlay {
        left: px.left as i32,
        top: px.top as i32,
        scene: Scene {
            form: "overlay".into(),
            theme: card.theme,
            width: w,
            height: h,
            card: Rect::from_wh(w, h),
            boxed: false,
            plot: Rect::from_wh(w, h),
            y_axis: false,
            items: items.into_iter().map(|i| shifted(i, -dx, -dy)).collect(),
        },
    }
}

/// Where the tooltip of `lines` goes beside the mark's outline `ring`:
/// centred over it, above it when there is room on the card and below it
/// otherwise, and inside the tile.
fn tip_box(fonts: &Fonts, ring: Rect, lines: &[(&str, Style)], card: &Scene) -> Rect {
    let text_w = lines
        .iter()
        .map(|(l, s)| fonts.width(l, *s))
        .fold(0.0, f32::max);
    #[allow(clippy::cast_precision_loss)] // At most MAX_TIP_LINES.
    let h = lines.len() as f32 * TIP_LINE + 2.0 * TIP_PAD.1;
    let w = (text_w + 2.0 * TIP_PAD.0).min(card.width);
    let above = ring.top - TIP_GAP - h;
    let top = if above >= 0.0 {
        above
    } else {
        (ring.bottom + TIP_GAP).min(card.height - h).max(0.0)
    };
    let left = (ring.center_x() - w / 2.0).min(card.width - w).max(0.0);
    Rect::from_xywh(left, top, w, h)
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
fn fit(fonts: &Fonts, text: &str, style: Style, max: f32) -> String {
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
