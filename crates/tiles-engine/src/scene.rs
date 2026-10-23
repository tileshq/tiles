//! The scene: every mark and text on the card, positioned in CSS pixels
//! (ADR-002). It stays in memory, so the raster, the entrance frames, the
//! hover hit map and the craft-rule tests all read the same thing.

use skia_safe::Rect;

use crate::Theme;
use crate::color::Rgba;
use crate::text::Style;

/// What an item is for. Rule tests select on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The headline.
    Title,
    /// The line under it.
    Subtitle,
    /// A faint horizontal gridline.
    Grid,
    /// A category or tick label on an axis.
    AxisLabel,
    /// A data mark: bar, point, line, area, dot, stem.
    Mark,
    /// A value written on or beside its mark.
    Value,
    /// A legend swatch or label.
    Legend,
    /// A connector, track or target tick that supports the marks.
    Guide,
    /// A patch of card colour behind a note's line, so no gridline, guide
    /// or fill crosses its letters (playbook M9).
    Knockout,
    /// An undrawn hover target over a value its mark draws without a point.
    Hit,
}

/// The colour role an item takes (playbook K1 to K6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// The accent: what the title names, or every mark when nothing is named.
    Accent,
    /// The red accent for a bad finding.
    Bad,
    /// Grey: marks the title does not name.
    Grey,
    /// Muted text.
    Muted,
    /// Ink text.
    Ink,
    /// The n-th colour of the fixed categorical order.
    Series(usize),
    /// The n-th neutral shade, for series the title does not name.
    Neutral(usize),
    /// The n-th context grey, for a line beside a named line (K1).
    Context(usize),
    /// Gridlines.
    Grid,
    /// Tracks behind marks.
    Track,
    /// The card's own colour, for a knockout.
    Card,
}

/// Where text hangs from its anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The point is the left end of the baseline.
    Start,
    /// The point is the middle of the baseline.
    Middle,
    /// The point is the right end of the baseline.
    End,
}

/// A rectangle's softened corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corners {
    /// Only the end away from the baseline is rounded (playbook M2).
    Top,
    /// Only the bottom end (a negative column).
    Bottom,
    /// Only the right end (a horizontal bar).
    Right,
    /// Only the left end (a negative horizontal bar).
    Left,
    /// All four.
    All,
}

/// The geometry of an item.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A rectangle with optionally rounded ends.
    Rect {
        /// Its box.
        rect: Rect,
        /// Corner radius.
        radius: f32,
        /// Which corners are rounded.
        corners: Corners,
    },
    /// A filled circle.
    Circle {
        /// Centre x.
        cx: f32,
        /// Centre y.
        cy: f32,
        /// Radius.
        r: f32,
    },
    /// A stroked polyline.
    Line {
        /// The points in order.
        points: Vec<(f32, f32)>,
        /// Stroke width.
        width: f32,
    },
    /// A filled area from a polyline down to a baseline.
    Area {
        /// The top edge.
        points: Vec<(f32, f32)>,
        /// The baseline y.
        base: f32,
    },
    /// One line of text.
    Text {
        /// The text.
        text: String,
        /// Anchor x.
        x: f32,
        /// Baseline y.
        y: f32,
        /// How it is set.
        style: Style,
        /// How it hangs from x.
        anchor: Anchor,
    },
}

/// How a mark enters (playbook N1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grow {
    /// Shown with the card.
    Still,
    /// Grows vertically from the baseline at this y.
    FromY(f32),
    /// Grows horizontally from the baseline at this x.
    FromX(f32),
    /// Revealed left to right across the plot.
    Wipe,
    /// Fades in as the marks land.
    Fade,
}

/// One thing drawn on the card.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// What it is for.
    pub part: Part,
    /// Its geometry.
    pub shape: Shape,
    /// Its colour role.
    pub emphasis: Emphasis,
    /// Its colour, resolved from the theme.
    pub color: Rgba,
    /// The row it draws, if any.
    pub datum: Option<usize>,
    /// Whether the intent's highlight names it.
    pub highlighted: bool,
    /// How it enters.
    pub grow: Grow,
    /// The hover text for a data mark.
    pub tip: Option<String>,
}

impl Item {
    /// A static item with no datum.
    #[must_use]
    pub fn new(part: Part, shape: Shape, emphasis: Emphasis, color: Rgba) -> Self {
        Self {
            part,
            shape,
            emphasis,
            color,
            datum: None,
            highlighted: false,
            grow: Grow::Still,
            tip: None,
        }
    }

    /// The same item drawing row `row`, with hover text.
    #[must_use]
    pub fn datum(mut self, row: usize, tip: String) -> Self {
        self.datum = Some(row);
        self.tip = Some(tip);
        self
    }

    /// The same item, marked as named by the highlight.
    #[must_use]
    pub fn highlighted(mut self, on: bool) -> Self {
        self.highlighted = on;
        self
    }

    /// The same item with an entrance.
    #[must_use]
    pub fn grow(mut self, grow: Grow) -> Self {
        self.grow = grow;
        self
    }

    /// The text of a text item.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match &self.shape {
            Shape::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The item's box, without stroke width or text extent.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match &self.shape {
            Shape::Rect { rect, .. } => *rect,
            Shape::Circle { cx, cy, r } => Rect::from_ltrb(cx - r, cy - r, cx + r, cy + r),
            Shape::Line { points, .. } | Shape::Area { points, .. } => {
                let mut r = Rect::from_ltrb(f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y) in points {
                    r.left = r.left.min(*x);
                    r.right = r.right.max(*x);
                    r.top = r.top.min(*y);
                    r.bottom = r.bottom.max(*y);
                }
                if let Shape::Area { base, .. } = &self.shape {
                    r.bottom = r.bottom.max(*base);
                }
                r
            }
            Shape::Text { x, y, style, .. } => Rect::from_ltrb(*x, y - style.size, *x, *y),
        }
    }
}

/// A laid-out card.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// The form drawn.
    pub form: String,
    /// The theme.
    pub theme: Theme,
    /// Tile width, margins included.
    pub width: f32,
    /// Tile height, margins included.
    pub height: f32,
    /// The card's box inside the tile.
    pub card: Rect,
    /// Whether the card's box, border and shadow are drawn; a dashboard's
    /// heading is text alone ([`crate::Engine::heading`]).
    pub boxed: bool,
    /// The plot area the marks sit in.
    pub plot: Rect,
    /// Whether the chart draws a y axis with tick labels.
    pub y_axis: bool,
    /// Everything on the card, in paint order.
    pub items: Vec<Item>,
}

impl Scene {
    /// The data marks.
    pub fn marks(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.part == Part::Mark)
    }

    /// Items of one part.
    pub fn part(&self, part: Part) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| i.part == part)
    }
}

/// Several laid-out cards on one picture, such as a row of scorecards.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// Picture width in CSS pixels.
    pub width: f32,
    /// Picture height in CSS pixels.
    pub height: f32,
    /// The cards, each at its offset.
    pub tiles: Vec<Placed>,
}

/// One card on a [`Board`].
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The card.
    pub scene: Scene,
    /// Its left edge on the board, in CSS pixels.
    pub x: f32,
    /// Its top edge on the board, in CSS pixels.
    pub y: f32,
}
