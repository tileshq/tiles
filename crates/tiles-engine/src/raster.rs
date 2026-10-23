//! Scene to pixels with Skia's CPU raster (ADR-002): the card chrome
//! (playbook C1 to C6), then every item, at an explicit device scale.

use skia_safe::{
    AlphaType, BlurStyle, ClipOp, Color, ColorType, EncodedImageFormat, ImageInfo, MaskFilter,
    Matrix, Paint, PaintCap, PaintJoin, PaintStyle, Path, PathBuilder, RRect, Rect, Vector,
    surfaces,
};

use crate::color::Rgba;
use crate::motion::Moment;
use crate::scene::{Anchor, Board, Corners, Grow, Item, Part, Scene, Shape};
use crate::text::{Fonts, Shaped};
use crate::tokens::Tokens;

fn color(c: Rgba) -> Color {
    Color::from_argb(c.a, c.r, c.g, c.b)
}

fn paint(c: Rgba) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(color(c));
    p
}

/// The scene with every text outline shaped once, ready to draw any number
/// of frames.
pub(crate) struct Prepared<'a> {
    scene: &'a Scene,
    tokens: &'a Tokens,
    // One entry per item: the shaped text already moved to its anchor.
    text: Vec<Option<Shaped>>,
}

impl<'a> Prepared<'a> {
    pub(crate) fn new(scene: &'a Scene, tokens: &'a Tokens, fonts: &Fonts) -> Self {
        let text = scene
            .items
            .iter()
            .map(|item| match &item.shape {
                Shape::Text {
                    text,
                    x,
                    y,
                    style,
                    anchor,
                } => {
                    let outline = fonts.outline(text, *style);
                    let w = fonts.width(text, *style);
                    let dx = match anchor {
                        Anchor::Start => *x,
                        Anchor::Middle => x - w / 2.0,
                        Anchor::End => x - w,
                    };
                    Some(outline.with_offset((dx, *y)))
                }
                _ => None,
            })
            .collect();
        Self {
            scene,
            tokens,
            text,
        }
    }

    /// Pixel size of the tile at `scale`.
    #[allow(clippy::cast_possible_truncation)] // Card sizes are far below i32::MAX.
    pub(crate) fn size(&self, scale: f32) -> (i32, i32) {
        (
            (self.scene.width * scale).round() as i32,
            (self.scene.height * scale).round() as i32,
        )
    }

    /// Draws the tile at one moment of the entrance and returns
    /// unpremultiplied RGBA pixels.
    pub(crate) fn rgba(&self, scale: f32, moment: Moment) -> Option<Vec<u8>> {
        let (w, h) = self.size(scale);
        let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
        let mut surface = surfaces::raster(&info, None, None)?;
        self.draw(surface.canvas(), scale, moment);
        let row = usize::try_from(w).ok()? * 4;
        let mut out = vec![0; row * usize::try_from(h).ok()?];
        surface
            .read_pixels(&info, &mut out, row, (0, 0))
            .then_some(out)
    }

    /// Draws the settled tile and encodes it as PNG.
    pub(crate) fn png(&self, scale: f32) -> Option<Vec<u8>> {
        let (w, h) = self.size(scale);
        let mut surface = surfaces::raster_n32_premul((w, h))?;
        self.draw(surface.canvas(), scale, Moment::FINAL);
        let image = surface.image_snapshot();
        let data = image.encode(None, EncodedImageFormat::PNG, None)?;
        Some(data.as_bytes().to_vec())
    }

    fn draw(&self, canvas: &skia_safe::Canvas, scale: f32, m: Moment) {
        canvas.clear(Color::TRANSPARENT);
        self.draw_over(canvas, scale, m);
    }

    /// Draws the tile over what the canvas already holds.
    fn draw_over(&self, canvas: &skia_safe::Canvas, scale: f32, m: Moment) {
        canvas.save();
        canvas.scale((scale, scale));
        let card = self.scene.card;
        // The card floats up and scales about its centre (CSS transform-origin).
        let (cx, cy) = (card.center_x(), card.center_y());
        let mut matrix = Matrix::translate((0.0, m.lift));
        matrix.pre_scale((m.scale, m.scale), Some((cx, cy).into()));
        canvas.concat(&matrix);
        let layer = if m.opacity < 1.0 {
            Some(canvas.save_layer_alpha_f(None, m.opacity))
        } else {
            None
        };
        if self.scene.boxed {
            self.chrome(canvas);
        }
        for (item, text) in self.scene.items.iter().zip(&self.text) {
            self.item(canvas, item, text.as_ref(), m);
        }
        if let Some(n) = layer {
            canvas.restore_to_count(n);
        }
        canvas.restore();
    }

    fn chrome(&self, canvas: &skia_safe::Canvas) {
        let t = &self.tokens.card;
        let pal = self.tokens.palette(self.scene.theme);
        let snap = Snap::of(canvas);
        let card = snap.map_or(self.scene.card, |s| s.rect(self.scene.card));
        let border = snap.map_or(t.border, |s| s.width(t.border, false));
        let outer = RRect::new_rect_xy(card, t.radius, t.radius);
        for s in &pal.shadows {
            let mut p = paint(Rgba::black(s.alpha));
            // A CSS blur radius is twice the Gaussian sigma.
            p.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, s.blur / 2.0, true));
            canvas.draw_rrect(
                RRect::new_rect_xy(card.with_offset((0.0, s.dy)), t.radius, t.radius),
                &p,
            );
        }
        canvas.draw_rrect(outer, &paint(pal.line));
        let inner = card.with_inset((border, border));
        let r = t.radius - border;
        canvas.draw_rrect(RRect::new_rect_xy(inner, r, r), &paint(pal.card));
    }

    #[allow(clippy::many_single_char_names, clippy::too_many_lines)] // Geometry: x, y, r, p, m are the usual names; one arm per shape.
    fn item(&self, canvas: &skia_safe::Canvas, item: &Item, text: Option<&Shaped>, m: Moment) {
        if item.part == Part::Hit {
            return;
        }
        let opacity = match item.grow {
            Grow::Fade if item.part == Part::Mark => m.grow,
            Grow::Fade => m.fade,
            _ => 1.0,
        };
        if opacity <= 0.0 {
            return;
        }
        let mut p = paint(item.color.with_opacity(opacity));
        let save = canvas.save();
        match item.grow {
            Grow::FromY(base) => {
                // A circle moves from the baseline instead of squashing (below).
                if !matches!(item.shape, Shape::Circle { .. }) {
                    let mut mx = Matrix::new_identity();
                    mx.set_scale((1.0, m.grow), Some((0.0, base).into()));
                    canvas.concat(&mx);
                }
            }
            Grow::FromX(base) => {
                if !matches!(item.shape, Shape::Circle { .. }) {
                    let mut mx = Matrix::new_identity();
                    mx.set_scale((m.grow, 1.0), Some((base, 0.0).into()));
                    canvas.concat(&mx);
                }
            }
            Grow::Wipe if m.grow < 1.0 => {
                let plot = self.scene.plot;
                let reveal = plot.left - 20.0 + (plot.width() + 40.0) * m.grow;
                canvas.clip_rect(
                    Rect::from_ltrb(-1e4, -1e4, reveal, 1e4),
                    ClipOp::Intersect,
                    true,
                );
            }
            _ => {}
        }
        if m.grow <= 0.0 && matches!(item.grow, Grow::FromX(_) | Grow::FromY(_) | Grow::Wipe) {
            canvas.restore_to_count(save);
            return;
        }
        let snap = Snap::of(canvas);
        match &item.shape {
            Shape::Rect {
                rect,
                radius,
                corners,
            } => {
                let rect = &snap.map_or(*rect, |s| s.rect(*rect));
                let r = radius
                    .min(rect.width() / 2.0)
                    .min(rect.height() / 2.0)
                    .max(0.0);
                let v = Vector::new(r, r);
                let z = Vector::new(0.0, 0.0);
                // Skia's corner order: top-left, top-right, bottom-right, bottom-left.
                let radii = match corners {
                    Corners::Top => [v, v, z, z],
                    Corners::Bottom => [z, z, v, v],
                    Corners::Right => [z, v, v, z],
                    Corners::Left => [v, z, z, v],
                    Corners::All => [v, v, v, v],
                };
                canvas.draw_rrect(RRect::new_rect_radii(rect, &radii), &p);
            }
            Shape::Circle { cx, cy, r } => {
                let (mut x, mut y) = (*cx, *cy);
                match item.grow {
                    Grow::FromX(base) => x = base + (x - base) * m.grow,
                    Grow::FromY(base) => y = base + (y - base) * m.grow,
                    _ => {}
                }
                canvas.draw_circle((x, y), *r, &p);
            }
            Shape::Line { points, width } => {
                if points.len() > 1 {
                    let butt = item.part != Part::Mark;
                    let (points, width) = match snap {
                        Some(s) => s.stroke(points, *width, butt),
                        None => (points.clone(), *width),
                    };
                    p.set_style(PaintStyle::Stroke);
                    p.set_stroke_width(width);
                    p.set_stroke_join(PaintJoin::Round);
                    p.set_stroke_cap(if butt {
                        PaintCap::Butt
                    } else {
                        PaintCap::Round
                    });
                    canvas.draw_path(&polyline(&points, None), &p);
                }
            }
            Shape::Area { points, base } => {
                if points.len() > 1 {
                    let base = snap.map_or(*base, |s| s.y(*base));
                    canvas.draw_path(&polyline(points, Some(base)), &p);
                }
            }
            Shape::Text { y, .. } => {
                if let Some(shaped) = text {
                    // Glyphs sit on a whole device pixel, as a browser sets them.
                    let dy = snap.map_or(0.0, |s| s.y(*y) - y);
                    if dy == 0.0 {
                        canvas.draw_path(&shaped.outline, &p);
                    } else {
                        canvas.draw_path(&shaped.outline.with_offset((0.0, dy)), &p);
                    }
                    // A colour emoji keeps its own colours and takes only the
                    // item's fade.
                    let mut picture = Paint::default();
                    picture.set_anti_alias(true);
                    picture.set_alpha_f(opacity * f32::from(item.color.a) / 255.0);
                    for run in &shaped.colour {
                        canvas.draw_glyphs_at(
                            &run.glyphs,
                            &run.points[..],
                            (0.0, dy),
                            &run.font,
                            &picture,
                        );
                    }
                }
            }
        }
        canvas.restore_to_count(save);
    }
}

/// Every card of a board with its text shaped once, ready to draw the
/// settled picture or any moment of the entrance.
pub(crate) struct PreparedBoard<'a> {
    board: &'a Board,
    tiles: Vec<Prepared<'a>>,
}

impl<'a> PreparedBoard<'a> {
    pub(crate) fn new(board: &'a Board, tokens: &'a Tokens, fonts: &Fonts) -> Self {
        Self {
            board,
            tiles: board
                .tiles
                .iter()
                .map(|t| Prepared::new(&t.scene, tokens, fonts))
                .collect(),
        }
    }

    /// Draws every card at its offset at moment `m` and encodes the picture
    /// as PNG, at zlib level `zlib` when given (else Skia's default).
    /// Offsets are rounded to whole device pixels, so each card's edges stay
    /// as crisp as when it is drawn alone.
    pub(crate) fn png(&self, scale: f32, m: Moment, zlib: Option<i32>) -> Option<Vec<u8>> {
        self.png_at(scale, &vec![m; self.tiles.len()], zlib)
    }

    /// As [`PreparedBoard::png`], with each card at its own moment, in the
    /// board's order.
    pub(crate) fn png_at(&self, scale: f32, at: &[Moment], zlib: Option<i32>) -> Option<Vec<u8>> {
        #[allow(clippy::cast_possible_truncation)] // Board sizes are far below i32::MAX.
        let size = (
            (self.board.width * scale).round() as i32,
            (self.board.height * scale).round() as i32,
        );
        let mut surface = surfaces::raster_n32_premul(size)?;
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        for ((tile, prepared), m) in self.board.tiles.iter().zip(&self.tiles).zip(at) {
            canvas.save();
            canvas.translate(((tile.x * scale).round(), (tile.y * scale).round()));
            prepared.draw_over(canvas, scale, *m);
            canvas.restore();
        }
        let image = surface.image_snapshot();
        let data = match zlib {
            None => image.encode(None, EncodedImageFormat::PNG, None)?,
            Some(level) => {
                let mut options = skia_safe::png_encoder::Options::default();
                options.z_lib_level = level;
                options.filter_flags = skia_safe::png_encoder::FilterFlag::SUB;
                skia_safe::png_encoder::encode_image(None, &image, &options)?
            }
        };
        Some(data.as_bytes().to_vec())
    }
}

/// Draws every card of `board` at its offset, settled, and encodes the
/// picture as PNG.
pub(crate) fn board_png(
    board: &Board,
    tokens: &Tokens,
    fonts: &Fonts,
    scale: f32,
) -> Option<Vec<u8>> {
    PreparedBoard::new(board, tokens, fonts).png(scale, Moment::FINAL, None)
}

/// The canvas's current transform when it only scales and moves, used to
/// put edges and hairlines on whole device pixels at any scale, so a 1 px
/// gridline draws as one solid row instead of two faint ones.
#[derive(Clone, Copy)]
struct Snap {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl Snap {
    fn of(canvas: &skia_safe::Canvas) -> Option<Self> {
        let m = canvas.local_to_device_as_3x3();
        let plain = !m.has_perspective() && m.skew_x() == 0.0 && m.skew_y() == 0.0;
        (plain && m.scale_x() > 0.0 && m.scale_y() > 0.0).then(|| Self {
            sx: m.scale_x(),
            sy: m.scale_y(),
            tx: m.translate_x(),
            ty: m.translate_y(),
        })
    }

    /// `x` moved to the nearest device pixel edge.
    fn x(self, x: f32) -> f32 {
        ((x * self.sx + self.tx).round() - self.tx) / self.sx
    }

    /// `y` moved to the nearest device pixel edge.
    fn y(self, y: f32) -> f32 {
        ((y * self.sy + self.ty).round() - self.ty) / self.sy
    }

    /// A stroke width rounded to whole device pixels, at least one.
    fn width(self, w: f32, vertical: bool) -> f32 {
        let s = if vertical { self.sx } else { self.sy };
        (w * s).round().max(1.0) / s
    }

    /// The centre of a stroke `w` device pixels wide across `v`, so both of
    /// its edges land on pixel edges.
    fn centre(v: f32, scale: f32, shift: f32, w: f32) -> f32 {
        let d = v * scale + shift;
        #[allow(clippy::cast_possible_truncation)] // A device stroke width of a few pixels.
        let odd = (w.round() as i64) % 2 == 1;
        let c = if odd { d.floor() + 0.5 } else { d.round() };
        (c - shift) / scale
    }

    /// Every edge of a box on a device pixel edge; a box thinner than a
    /// pixel keeps one pixel rather than vanishing.
    fn rect(self, r: Rect) -> Rect {
        let (mut l, mut t, mut rr, mut b) = (
            self.x(r.left),
            self.y(r.top),
            self.x(r.right),
            self.y(r.bottom),
        );
        if r.width() > 0.0 && rr <= l {
            rr = l + 1.0 / self.sx;
        }
        if r.height() > 0.0 && b <= t {
            b = t + 1.0 / self.sy;
        }
        if r.width() < 0.0 {
            std::mem::swap(&mut l, &mut rr);
        }
        if r.height() < 0.0 {
            std::mem::swap(&mut t, &mut b);
        }
        Rect::from_ltrb(l, t, rr, b)
    }

    /// A polyline made only of level and upright runs (gridlines, guides,
    /// connectors, stems) with its width in whole device pixels and centred
    /// so both edges land on pixel edges; butt ends move to pixel edges too.
    /// A sloped line is a data line and stays where the data puts it.
    #[allow(clippy::float_cmp)] // A level run's ends are built from one y, so they are equal exactly.
    fn stroke(self, points: &[(f32, f32)], w: f32, butt: bool) -> (Vec<(f32, f32)>, f32) {
        let level = points.windows(2).all(|p| p[0].1 == p[1].1);
        let upright = points.windows(2).all(|p| p[0].0 == p[1].0);
        if !(level || upright) {
            return (points.to_vec(), w);
        }
        let width = self.width(w, upright && !level);
        let out = points
            .iter()
            .map(|&(x, y)| {
                if level {
                    let y = Self::centre(y, self.sy, self.ty, width * self.sy);
                    (if butt { self.x(x) } else { x }, y)
                } else {
                    let x = Self::centre(x, self.sx, self.tx, width * self.sx);
                    (x, if butt { self.y(y) } else { y })
                }
            })
            .collect();
        (out, width)
    }
}

fn polyline(points: &[(f32, f32)], base: Option<f32>) -> Path {
    let mut b = PathBuilder::new();
    b.move_to(points[0]);
    for pt in &points[1..] {
        b.line_to(*pt);
    }
    if let Some(base) = base {
        let last = points[points.len() - 1];
        b.line_to((last.0, base));
        b.line_to((points[0].0, base));
        b.close();
    }
    b.detach()
}
